//! 프로젝트가 쓰는 레포 키 — 시크릿 저장소의 배포 키를 그대로 쓴다.
//!
//! 키를 만들고 GitHub 에 등록하는 일은 배포 키 절차(`Keyring`)가 이미 한다. 여기서는 그
//! 절차를 부르고, 프로젝트가 알아야 하는 것(용도 · 등록 여부 · 개인 키 파일 자리)만 넘긴다.
//! 지우는 길은 두지 않는다.

use secrets_core::key::{DeployKey, KeyRef, KeyState, Keyring, RepoRef};
use secrets_core::port::ProgressSink;
use secrets_core::project::{ProjectError, RepoKey, RepoKeys};

use crate::adapter::clock::SystemClock;
use crate::keys::{FileKeys, GhKeys, paths};

pub struct VaultRepoKeys;

fn keyring() -> Keyring<'static> {
    static GH: GhKeys = GhKeys;
    static VAULT: FileKeys = FileKeys;
    static CLOCK: SystemClock = SystemClock;
    Keyring::new(&GH, &VAULT, &CLOCK)
}

/// GitHub 은 소유자와 이름의 대소문자를 가리지 않는다.
fn same_repo(a: &str, b: &RepoRef) -> bool {
    a.eq_ignore_ascii_case(&b.slug())
}

fn repo_key(key: &DeployKey) -> Option<RepoKey> {
    let at = key.at()?;
    Some(RepoKey {
        purpose: key.purpose.clone(),
        account: key.account.clone(),
        write: key.write,
        usable: matches!(key.state, KeyState::Registered | KeyState::Rotating),
        private_key: paths::private_of(&at).display().to_string(),
    })
}

impl RepoKeys for VaultRepoKeys {
    fn keys_for(&self, repo: &RepoRef) -> Vec<RepoKey> {
        let mut keys: Vec<RepoKey> = keyring()
            .list()
            .into_iter()
            .filter_map(Result::ok)
            .filter(|k| same_repo(&k.repo, repo))
            .filter_map(|k| repo_key(&k))
            .collect();
        keys.sort_by(|a, b| a.purpose.cmp(&b.purpose));
        keys
    }

    fn issue(
        &self,
        account: &str,
        repo: &RepoRef,
        purpose: &str,
        progress: &dyn ProgressSink,
    ) -> Result<RepoKey, ProjectError> {
        let at = KeyRef::new(repo.clone(), purpose)
            .ok_or_else(|| ProjectError::Invalid(format!("{purpose}은(는) 키 용도로 쓸 수 없습니다.")))?;
        let key = keyring()
            .create(account, &at, true, progress)
            .map_err(|e| ProjectError::Storage(e.to_string()))?;
        repo_key(&key).ok_or_else(|| ProjectError::Storage(format!("{} 키 기록을 읽지 못했습니다.", at.slug())))
    }
}
