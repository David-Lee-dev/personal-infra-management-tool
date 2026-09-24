//! 개인 키와 기록이 실제로 놓이는 곳.
//!
//! 키 쌍은 `ssh-keygen` 이 만든다. 직접 구현하지 않는다 — 암호를 직접 짜는 것은
//! 이 도구가 할 일이 아니고, 만들어진 키가 표준 도구와 똑같아야 한다.

use std::io;
use std::path::Path;

use secrets_core::credential::secret::Secret;
use secrets_core::key::{DeployKey, KeyError, KeyRef, KeyVault, Material, RepoRef};

use crate::cli::{exec, tools};
use crate::clock;
use crate::keys::paths::{self, PRIVATE, PUBLIC};
use crate::vault;

pub struct FileKeys;

fn storage(e: impl std::fmt::Display) -> KeyError {
    KeyError::Storage(e.to_string())
}

impl FileKeys {
    /// 대기 자리에 키 쌍을 만든다.
    fn keygen(&self, at: &KeyRef, comment: &str) -> Result<Material, KeyError> {
        let staging = paths::staging_of(at);
        // 지난번 시도가 남아 있으면 ssh-keygen 이 덮어쓸지 물어보며 멈춘다.
        let _ = std::fs::remove_dir_all(&staging);
        vault::create_private(&staging).map_err(storage)?;

        let target = staging.join(PRIVATE);
        let program = tools::find_in_path("ssh-keygen")
            .ok_or_else(|| KeyError::Storage("ssh-keygen 을 찾을 수 없습니다".into()))?;

        let outcome = exec::run(
            &program,
            &[
                "-t",
                "ed25519",
                "-C",
                comment,
                // 사람이 칠 수 없는 자리에서 쓰는 키다. 암호구를 걸면 CI 가 멈춘다.
                "-N",
                "",
                "-f",
                &target.display().to_string(),
            ],
            |_, _| {},
        )
        .map_err(storage)?;

        if !outcome.ok() {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(KeyError::Storage("키 쌍을 만들지 못했습니다".into()));
        }

        let public_key = read(&staging.join(PUBLIC))?;
        let (fingerprint, algorithm) = describe(&staging.join(PUBLIC))?;
        Ok(Material {
            public_key,
            fingerprint,
            algorithm,
        })
    }
}

/// `ssh-keygen -l` 이 말하는 지문과 알고리즘. 우리가 계산하지 않는다.
fn describe(public_path: &Path) -> Result<(String, String), KeyError> {
    let program = tools::find_in_path("ssh-keygen")
        .ok_or_else(|| KeyError::Storage("ssh-keygen 을 찾을 수 없습니다".into()))?;

    let buffer = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = buffer.clone();
    let outcome = exec::run(
        &program,
        &["-l", "-f", &public_path.display().to_string()],
        move |stream, line| {
            if stream == exec::Stream::Stdout {
                sink.lock().unwrap().push_str(&line);
            }
        },
    )
    .map_err(storage)?;

    if !outcome.ok() {
        return Err(KeyError::Storage("키 지문을 읽지 못했습니다".into()));
    }

    // `256 SHA256:… comment (ED25519)`
    let text = buffer.lock().unwrap().clone();
    let fingerprint = text
        .split_whitespace()
        .find(|part| part.starts_with("SHA256:"))
        .ok_or_else(|| KeyError::Storage("키 지문을 읽지 못했습니다".into()))?
        .to_string();
    let algorithm = text
        .rsplit_once('(')
        .and_then(|(_, rest)| rest.strip_suffix(')'))
        .map(|a| a.trim().to_lowercase())
        .unwrap_or_else(|| "ed25519".into());

    Ok((fingerprint, algorithm))
}

fn read(path: &Path) -> Result<String, KeyError> {
    std::fs::read_to_string(path)
        .map(|t| t.trim_end().to_string())
        .map_err(storage)
}

/// 임시 파일에 다 쓴 뒤 제자리로 옮긴다. 같은 디렉토리 안이라 rename 이 원자적이다.
fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;

    let staging = path.with_extension("writing");
    {
        let mut file = std::fs::File::create(&staging)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    vault::restrict(&staging)?;

    if let Err(e) = std::fs::rename(&staging, path) {
        let _ = std::fs::remove_file(&staging);
        return Err(e);
    }
    Ok(())
}

impl KeyVault for FileKeys {
    fn exists(&self, at: &KeyRef) -> bool {
        paths::record_of(at).is_file()
    }

    fn stage(&self, at: &KeyRef, comment: &str) -> Result<Material, KeyError> {
        self.keygen(at, comment)
    }

    fn place(&self, at: &KeyRef) -> Result<(), KeyError> {
        let dir = paths::dir_of(at);
        let staging = paths::staging_of(at);
        if !staging.join(PRIVATE).is_file() {
            return Err(KeyError::Missing(at.slug()));
        }

        // 제자리에 키가 있으면 먼저 이력으로 물린다. 덮어쓰면 옛 키를 잃는다.
        if paths::private_of(at).is_file() {
            let kept = paths::history_of(at).join(clock::stamp());
            vault::create_private(&kept).map_err(storage)?;
            for name in [PRIVATE, PUBLIC] {
                std::fs::rename(dir.join(name), kept.join(name)).map_err(storage)?;
            }
        }

        for name in [PRIVATE, PUBLIC] {
            std::fs::rename(staging.join(name), dir.join(name)).map_err(storage)?;
        }
        let _ = std::fs::remove_dir_all(&staging);
        vault::restrict(&paths::private_of(at)).map_err(storage)
    }

    fn discard_staged(&self, at: &KeyRef) {
        let _ = std::fs::remove_dir_all(paths::staging_of(at));
    }

    fn record(&self, key: &DeployKey) -> Result<(), KeyError> {
        let at = key.at().ok_or_else(|| KeyError::Storage(format!("{}의 저장 위치를 읽지 못했습니다.", key.repo)))?;
        vault::create_private(&paths::dir_of(&at)).map_err(storage)?;

        let text = toml::to_string_pretty(key).map_err(storage)?;
        write_atomically(&paths::record_of(&at), text.as_bytes()).map_err(storage)
    }

    fn load(&self, at: &KeyRef) -> Result<DeployKey, KeyError> {
        let text = std::fs::read_to_string(paths::record_of(at))
            .map_err(|_| KeyError::Missing(at.slug()))?;
        toml::from_str(&text).map_err(storage)
    }

    /// 디렉토리를 훑어 기록을 모은다. 읽지 못한 것은 건너뛰지 않고 오류로 남긴다.
    fn list(&self) -> Vec<Result<DeployKey, String>> {
        let mut found = Vec::new();
        let root = paths::github_root();

        for owner in dirs_in(&root) {
            for repo in dirs_in(&root.join(&owner)) {
                let Some(target) = RepoRef::parse(&format!("{owner}/{repo}")) else {
                    continue;
                };
                for purpose in dirs_in(&root.join(&owner).join(&repo)) {
                    let Some(at) = KeyRef::new(target.clone(), &purpose) else {
                        continue;
                    };
                    if !self.exists(&at) {
                        continue;
                    }
                    found.push(
                        self.load(&at)
                            .map_err(|e| format!("{} 를 읽지 못했습니다: {e}", at.slug())),
                    );
                }
            }
        }
        found
    }

    fn public_key(&self, at: &KeyRef) -> Result<String, KeyError> {
        read(&paths::public_of(at))
    }

    fn staged_public_key(&self, at: &KeyRef) -> Result<String, KeyError> {
        read(&paths::staging_of(at).join(PUBLIC))
    }

    fn private_key(&self, at: &KeyRef) -> Result<Secret, KeyError> {
        Ok(Secret::new(
            std::fs::read_to_string(paths::private_of(at)).map_err(storage)?,
        ))
    }

    fn move_to(&self, from: &KeyRef, to: &KeyRef) -> Result<(), KeyError> {
        let target = paths::dir_of(to);
        if let Some(parent) = target.parent() {
            vault::create_private(parent).map_err(storage)?;
        }
        // rename 은 대상이 있으면 덮어쓴다. 남의 키를 지우지 않게 먼저 막는다.
        if target.exists() {
            return Err(KeyError::Taken(to.slug()));
        }
        std::fs::rename(paths::dir_of(from), &target).map_err(storage)
    }

    fn archive(&self, at: &KeyRef, reason: &str) -> Result<(), KeyError> {
        let stamp = clock::stamp();
        let kept = paths::archive_of(at, &stamp);
        if let Some(parent) = kept.parent() {
            vault::create_private(parent).map_err(storage)?;
        }

        // 무엇을 왜 걷어냈는지 남긴다. 이유 없는 보관은 나중에 판단할 수 없다.
        let dir = paths::dir_of(at);
        let note = format!("archived_at = \"{}\"\nreason = \"{reason}\"\n", clock::now());
        let _ = std::fs::write(dir.join("archived.toml"), note);

        std::fs::rename(&dir, &kept).map_err(storage)
    }
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
