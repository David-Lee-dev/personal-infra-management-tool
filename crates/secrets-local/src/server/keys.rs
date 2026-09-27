//! 계정 키 파일 — 이 도구가 만든 키, 가져온 키, pem, 로컬 키 파일.
//!
//! 시크릿 저장소 안의 키는 저장소 뿌리 기준 상대 경로로 기록한다. 새로 만드는 키는
//! `keys/server/<id>/<계정>/` 에 둔다. 서버 기록이 생기기 전에 만든 키는 원래 자리
//! (`keys/aws/…/instance/…`)에 그대로 있고 기록이 그 자리를 가리킨다 — 사용자가 직접 쓴
//! `~/.ssh/config` 가 그 경로를 가리키고 있어 옮기면 그 접속이 끊긴다.

use std::path::{Path, PathBuf};

use secrets_core::server::{AccountKey, AccountKeys, CreatedKey, Server, ServerError};

use crate::cli::{exec, tools};
use crate::{aws_vault, clock, vault};

pub const PRIVATE: &str = "key";
pub const PUBLIC: &str = "key.pub";

pub struct VaultKeys;

fn storage(e: impl std::fmt::Display) -> ServerError {
    ServerError::Storage(e.to_string())
}

/// `~/` 로 시작하면 홈으로 푼다.
pub fn expand(text: &str) -> PathBuf {
    match text.strip_prefix("~/") {
        Some(rest) => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(rest),
        None => PathBuf::from(text),
    }
}

/// 새로 만드는 키의 디렉토리 — 저장소 뿌리 기준 상대 경로.
fn relative_dir(server: &str, login: &str) -> Result<String, ServerError> {
    for part in [server, login] {
        if part.is_empty() || part.contains(['/', '\\']) || part.starts_with('.') {
            return Err(ServerError::Invalid(format!(
                "{part}은(는) 경로 이름으로 쓸 수 없습니다."
            )));
        }
    }
    Ok(format!("keys/server/{server}/{login}"))
}

/// 저장소 안의 상대 경로를 절대 경로로. 저장소 밖으로 나가는 경로는 받지 않는다.
fn in_vault(relative: &str) -> Result<PathBuf, ServerError> {
    let escapes = relative.starts_with('/')
        || Path::new(relative)
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir));
    if escapes {
        return Err(ServerError::Invalid(format!(
            "{relative}은(는) 시크릿 저장소 안의 경로가 아닙니다."
        )));
    }
    Ok(vault::root().join(relative))
}

impl VaultKeys {
    /// ssh 에 넘길 개인 키의 절대 경로. 파일이 있는지는 보지 않는다. `Agent` 면 없다.
    pub fn path_of(
        &self,
        server: &Server,
        key: &AccountKey,
    ) -> Result<Option<PathBuf>, ServerError> {
        Ok(match key {
            AccountKey::Agent => None,
            AccountKey::Vault { path } => Some(in_vault(path)?),
            AccountKey::File { path } => Some(expand(path)),
            AccountKey::Pem { keypair } => {
                let aws = server
                    .aws
                    .as_ref()
                    .filter(|_| server.kind.is_aws())
                    .ok_or_else(|| {
                        ServerError::Invalid(format!(
                            "{}에 AWS 계정 · 리전이 없어 pem 을 찾을 수 없습니다.",
                            server.name
                        ))
                    })?;
                Some(
                    aws_vault::dir_of(&aws.account, server.kind.id(), &aws.region, keypair)
                        .join(PRIVATE),
                )
            }
        })
    }
}

impl AccountKeys for VaultKeys {
    fn create(&self, server: &str, login: &str, comment: &str) -> Result<CreatedKey, ServerError> {
        let relative = relative_dir(server, login)?;
        let at = vault::root().join(&relative);
        vault::create_private(&at).map_err(storage)?;
        let target = at.join(PRIVATE);
        // 지난 시도가 남아 있으면 ssh-keygen 이 덮어쓸지 물으며 멈춘다.
        let _ = std::fs::remove_file(&target);
        let _ = std::fs::remove_file(at.join(PUBLIC));

        let program = tools::find_in_path("ssh-keygen")
            .ok_or_else(|| ServerError::Storage("ssh-keygen 을 찾을 수 없습니다".into()))?;
        let outcome = exec::run(
            &program,
            &[
                "-t",
                "ed25519",
                "-C",
                comment,
                // 사람이 칠 수 없는 자리에서 쓰는 키다. 암호구를 걸면 배포가 멈춘다.
                "-N",
                "",
                "-f",
                &target.display().to_string(),
            ],
            |_, _| {},
        )
        .map_err(storage)?;
        if !outcome.ok() {
            return Err(ServerError::Storage("키 쌍을 만들지 못했습니다".into()));
        }
        vault::restrict(&target).map_err(storage)?;

        let public_key = std::fs::read_to_string(at.join(PUBLIC))
            .map(|text| text.trim_end().to_string())
            .map_err(storage)?;
        let fingerprint = aws_vault::fingerprint_of(&target).map_err(storage)?;
        Ok(CreatedKey {
            key: AccountKey::Vault {
                path: format!("{relative}/{PRIVATE}"),
            },
            public_key,
            fingerprint,
        })
    }

    fn import(&self, server: &str, login: &str, source: &str) -> Result<AccountKey, ServerError> {
        let source = expand(source);
        let text = std::fs::read(&source).map_err(|e| {
            ServerError::Invalid(format!("{}을(를) 읽지 못했습니다: {e}", source.display()))
        })?;
        if !String::from_utf8_lossy(&text).contains("PRIVATE KEY") {
            return Err(ServerError::Invalid(format!(
                "{}은(는) 개인 키 파일이 아닙니다.",
                source.display()
            )));
        }
        let relative = relative_dir(server, login)?;
        let at = vault::root().join(&relative);
        vault::create_private(&at).map_err(storage)?;
        let target = at.join(PRIVATE);
        if target.exists() {
            return Err(ServerError::Taken(format!("{relative}/{PRIVATE}")));
        }
        vault::write_private(&target, &text).map_err(storage)?;
        Ok(AccountKey::Vault {
            path: format!("{relative}/{PRIVATE}"),
        })
    }

    fn public_key(&self, server: &Server, key: &AccountKey) -> Result<String, ServerError> {
        let private = self.path_of(server, key)?.ok_or_else(|| {
            ServerError::Invalid("ssh 기본 키의 공개 키는 알 수 없습니다.".into())
        })?;
        let public = PathBuf::from(format!("{}.pub", private.display()));
        std::fs::read_to_string(&public)
            .map(|text| text.trim_end().to_string())
            .map_err(|_| ServerError::Missing(public.display().to_string()))
    }

    fn private_path(
        &self,
        server: &Server,
        key: &AccountKey,
    ) -> Result<Option<String>, ServerError> {
        match self.path_of(server, key)? {
            None => Ok(None),
            Some(path) if path.is_file() => Ok(Some(path.display().to_string())),
            Some(path) => Err(ServerError::Missing(format!("키 파일 {}", path.display()))),
        }
    }

    fn discard(&self, key: &AccountKey) {
        let AccountKey::Vault { path } = key else {
            return;
        };
        let Ok(private) = in_vault(path) else { return };
        let Some(dir) = private.parent() else { return };
        let _ = std::fs::remove_dir_all(dir);
        // 이 키 때문에 생긴 빈 `<id>/` 도 걷는다. 비어 있지 않으면 remove_dir 가 실패하고 남는다.
        if let Some(parent) = dir.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }

    fn archive(&self, server: &str, key: &AccountKey) -> Result<(), ServerError> {
        // 저장소 밖의 파일 · pem 은 이 도구가 만든 계정 키가 아니다. 그대로 둔다.
        let AccountKey::Vault { path } = key else {
            return Ok(());
        };
        let private = in_vault(path)?;
        let Some(dir) = private.parent() else {
            return Ok(());
        };
        if !dir.is_dir() {
            return Ok(());
        }
        let login = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("account");
        let kept = vault::root()
            .join("archive")
            .join("keys")
            .join("server")
            .join(server)
            .join(format!("{login}-{}", clock::stamp()));
        if let Some(parent) = kept.parent() {
            vault::create_private(parent).map_err(storage)?;
        }
        std::fs::rename(dir, &kept).map_err(storage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::tests_support::with_temp_root;

    fn server(kind: &str, aws: bool) -> Server {
        let aws = if aws {
            "[aws]\naccount = \"320042238085\"\nregion = \"ap-northeast-2\"\n"
        } else {
            ""
        };
        toml::from_str(&format!(
            "id = \"s\"\nname = \"s\"\naddress = \"1.1.1.1\"\nkind = \"{kind}\"\nregistered_at = \"t\"\n{aws}"
        ))
        .unwrap()
    }

    #[test]
    fn each_kind_of_key_resolves_to_where_it_lives() {
        with_temp_root(|root| {
            let keys = VaultKeys;
            let ec2 = server("ec2", true);
            assert_eq!(keys.path_of(&ec2, &AccountKey::Agent).unwrap(), None);
            assert_eq!(
                keys.path_of(
                    &ec2,
                    &AccountKey::Vault {
                        path: "keys/server/s/deploy/key".into()
                    }
                )
                .unwrap(),
                Some(root.join("keys/server/s/deploy/key"))
            );
            assert_eq!(
                keys.path_of(
                    &ec2,
                    &AccountKey::Pem {
                        keypair: "tuk-key".into()
                    }
                )
                .unwrap(),
                Some(root.join("keys/aws/320042238085/ec2/ap-northeast-2/tuk-key/key"))
            );
            let home = std::env::var("HOME").unwrap();
            assert_eq!(
                keys.path_of(
                    &ec2,
                    &AccountKey::File {
                        path: "~/.ssh/nemo-mac".into()
                    }
                )
                .unwrap(),
                Some(PathBuf::from(home).join(".ssh/nemo-mac"))
            );
        });
    }

    #[test]
    fn a_pem_without_aws_facts_or_a_path_leaving_the_vault_is_refused() {
        with_temp_root(|_| {
            let keys = VaultKeys;
            assert!(
                keys.path_of(
                    &server("ec2", false),
                    &AccountKey::Pem {
                        keypair: "k".into()
                    }
                )
                .is_err()
            );
            assert!(
                keys.path_of(
                    &server("other", true),
                    &AccountKey::Pem {
                        keypair: "k".into()
                    }
                )
                .is_err()
            );
            assert!(
                keys.path_of(
                    &server("other", false),
                    &AccountKey::Vault {
                        path: "../x/key".into()
                    }
                )
                .is_err()
            );
        });
    }

    #[test]
    fn a_missing_key_file_is_named_before_ssh_is_tried() {
        with_temp_root(|_| {
            let keys = VaultKeys;
            let missing = AccountKey::Vault {
                path: "keys/server/s/deploy/key".into(),
            };
            assert!(matches!(
                keys.private_path(&server("other", false), &missing),
                Err(ServerError::Missing(_))
            ));
        });
    }

    #[test]
    fn a_created_key_is_private_and_archiving_moves_it_out() {
        with_temp_root(|root| {
            let keys = VaultKeys;
            let created = keys.create("s", "deploy", "secrets/s/deploy").unwrap();
            assert!(created.public_key.starts_with("ssh-ed25519 "));
            let private = root.join("keys/server/s/deploy/key");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    std::fs::metadata(&private).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
            assert_eq!(
                keys.public_key(&server("other", false), &created.key)
                    .unwrap(),
                created.public_key
            );

            keys.archive("s", &created.key).unwrap();
            assert!(!private.exists());
            assert!(root.join("archive/keys/server/s").is_dir());
        });
    }

    #[test]
    fn an_imported_key_is_copied_and_the_original_stays() {
        with_temp_root(|root| {
            std::fs::create_dir_all(root).unwrap();
            let source = root.join("id_test");
            std::fs::write(&source, "-----BEGIN OPENSSH PRIVATE KEY-----\nx\n").unwrap();
            let key = VaultKeys
                .import("s", "david", &source.display().to_string())
                .unwrap();
            assert_eq!(
                key,
                AccountKey::Vault {
                    path: "keys/server/s/david/key".into()
                }
            );
            assert!(source.is_file());
            assert!(root.join("keys/server/s/david/key").is_file());

            let text = root.join("notes.txt");
            std::fs::write(&text, "hello").unwrap();
            assert!(
                VaultKeys
                    .import("s", "other", &text.display().to_string())
                    .is_err()
            );
        });
    }
}
