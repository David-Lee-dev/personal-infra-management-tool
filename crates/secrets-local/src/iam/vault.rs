//! IAM 의 기록 · 정책 원문 · 시크릿이 놓이는 곳.
//!
//! ```text
//! keys/aws/<계정ID>/iam/<이름>/
//!   iam.toml       기록 — 키 ID, 소비처
//!   policy.json    AWS 에 붙인 정책 원문
//!   secret         시크릿 액세스 키  0600
//! ```
//!
//! 키 ID 와 시크릿을 한 파일에 두지 않는다. 기록은 화면이 늘 읽고, 시크릿은 넣을
//! 때만 읽는다.

use std::path::{Path, PathBuf};

use secrets_core::aws::iam::{IamError, IamRef, IamUser, IamVault};
use secrets_core::credential::secret::Secret;

use crate::clock;
use crate::vault;

pub const FILE: &str = "iam.toml";
pub const POLICY: &str = "policy.json";
pub const SECRET: &str = "secret";

pub struct FileIam;

fn storage(e: impl std::fmt::Display) -> IamError {
    IamError::Storage(e.to_string())
}

/// AWS 계정들이 모이는 뿌리. pem 과 같은 뿌리다.
fn aws_root() -> PathBuf {
    vault::root().join("keys").join("aws")
}

pub fn dir_of(at: &IamRef) -> PathBuf {
    aws_root().join(&at.account).join("iam").join(&at.name)
}

fn archive_of(at: &IamRef, stamp: &str) -> PathBuf {
    vault::root()
        .join("archive")
        .join("keys")
        .join("aws")
        .join(&at.account)
        .join("iam")
        .join(format!("{}-{stamp}", at.name))
}

/// 임시 파일에 다 쓴 뒤 제자리로 옮긴다. 같은 디렉토리 안이라 rename 이 원자적이다.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), IamError> {
    use std::io::Write;

    let staging = path.with_extension("writing");
    {
        let mut file = std::fs::File::create(&staging).map_err(storage)?;
        file.write_all(bytes).map_err(storage)?;
        file.sync_all().map_err(storage)?;
    }
    vault::restrict(&staging).map_err(storage)?;
    if let Err(e) = std::fs::rename(&staging, path) {
        let _ = std::fs::remove_file(&staging);
        return Err(storage(e));
    }
    Ok(())
}

fn read_secret(path: &Path) -> Result<Secret, IamError> {
    let text = std::fs::read_to_string(path).map_err(storage)?;
    Ok(Secret::new(text.trim()))
}

fn dirs_in(path: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

impl IamVault for FileIam {
    fn exists(&self, at: &IamRef) -> bool {
        dir_of(at).join(FILE).is_file()
    }

    fn names(&self, account: &str) -> Vec<String> {
        let root = aws_root().join(account).join("iam");
        dirs_in(&root)
            .into_iter()
            .filter(|name| root.join(name).join(FILE).is_file())
            .collect()
    }

    /// 읽지 못한 기록은 건너뛰지 않고 오류로 남긴다. 조용히 빠지면 IAM 이 사라진 것처럼 보인다.
    fn list(&self) -> Vec<Result<IamUser, String>> {
        let mut found = Vec::new();
        for account in dirs_in(&aws_root()) {
            for name in self.names(&account) {
                let at = IamRef { account: account.clone(), name };
                found.push(self.load(&at).map_err(|e| format!("{} 를 읽지 못했습니다: {e}", at.slug())));
            }
        }
        found
    }

    fn load(&self, at: &IamRef) -> Result<IamUser, IamError> {
        let text = std::fs::read_to_string(dir_of(at).join(FILE))
            .map_err(|_| IamError::Missing(at.slug()))?;
        toml::from_str(&text).map_err(storage)
    }

    fn record(&self, user: &IamUser) -> Result<(), IamError> {
        let dir = dir_of(&user.at());
        vault::create_private(&dir).map_err(storage)?;
        let text = toml::to_string_pretty(user).map_err(storage)?;
        write_atomically(&dir.join(FILE), text.as_bytes())
    }

    /// 시크릿과 정책을 먼저 쓰고 기록을 마지막에 쓴다. 기록이 있어야 목록에 뜨므로,
    /// 중간에 죽으면 목록에 없는 반쪽 디렉토리가 남을 뿐 반쪽 IAM 은 보이지 않는다.
    fn keep(&self, user: &IamUser, policy: &str, secret: &Secret) -> Result<(), IamError> {
        let dir = dir_of(&user.at());
        vault::create_private(&dir).map_err(storage)?;
        write_atomically(&dir.join(SECRET), secret.expose().as_bytes())?;
        write_atomically(&dir.join(POLICY), policy.as_bytes())?;
        self.record(user)
    }

    fn policy(&self, at: &IamRef) -> Result<String, IamError> {
        std::fs::read_to_string(dir_of(at).join(POLICY)).map_err(storage)
    }

    fn secret(&self, at: &IamRef) -> Result<Secret, IamError> {
        read_secret(&dir_of(at).join(SECRET))
    }

    fn archive(&self, at: &IamRef, reason: &str) -> Result<(), IamError> {
        let kept = archive_of(at, &clock::stamp());
        if let Some(parent) = kept.parent() {
            vault::create_private(parent).map_err(storage)?;
        }
        // 무엇을 왜 걷어냈는지 남긴다. 이유 없는 보관은 나중에 판단할 수 없다.
        let dir = dir_of(at);
        let note = format!("걷어낸 시각 = \"{}\"\n이유 = \"{reason}\"\n", clock::now());
        let _ = std::fs::write(dir.join("archived.toml"), note);
        std::fs::rename(&dir, &kept).map_err(storage)
    }

    fn discard(&self, at: &IamRef) {
        let _ = std::fs::remove_dir_all(dir_of(at));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::tests_support::with_temp_root;

    fn user(name: &str) -> IamUser {
        IamUser {
            name: name.into(),
            app: "tuk-api".into(),
            env: "prod".into(),
            perm: "s3".into(),
            purpose: String::new(),
            account: "123".into(),
            master: "admin".into(),
            key_id: "KEY1".into(),
            issued_at: "t".into(),
            created_at: "t".into(),
            consumers: Vec::new(),
            checked: None,
            deletable_from: String::new(),
        }
    }

    #[test]
    fn a_kept_iam_is_listed_with_its_secret_readable_only_by_the_owner() {
        with_temp_root(|_| {
            let vault = FileIam;
            let it = user("tuk-api-prod-s3-iam");
            vault.keep(&it, "{}", &Secret::new("s1")).unwrap();

            let listed: Vec<String> = vault.list().into_iter().map(|r| r.unwrap().name).collect();
            assert_eq!(listed, vec!["tuk-api-prod-s3-iam"]);
            assert_eq!(vault.secret(&it.at()).unwrap().expose(), "s1");

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(dir_of(&it.at()).join(SECRET)).unwrap().permissions().mode();
                assert_eq!(mode & 0o777, 0o600);
            }
        });
    }

    #[test]
    fn a_directory_without_a_record_is_not_an_iam() {
        with_temp_root(|_| {
            let vault = FileIam;
            let half = IamRef { account: "123".into(), name: "half-prod-s3-iam".into() };
            vault::create_private(&dir_of(&half)).unwrap();
            std::fs::write(dir_of(&half).join(SECRET), "s").unwrap();

            assert!(vault.names("123").is_empty());
            assert!(!vault.exists(&half));
        });
    }

    #[test]
    fn archiving_moves_everything_out_of_the_list() {
        with_temp_root(|root| {
            let vault = FileIam;
            let it = user("gone-prod-s3-iam");
            vault.keep(&it, "{}", &Secret::new("s")).unwrap();

            vault.archive(&it.at(), "삭제").unwrap();

            assert!(vault.list().is_empty());
            let archived = root.join("archive/keys/aws/123/iam");
            let entry = std::fs::read_dir(&archived).unwrap().next().unwrap().unwrap().path();
            assert!(entry.join(SECRET).is_file());
            assert!(entry.join("archived.toml").is_file());
        });
    }
}
