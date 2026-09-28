//! 배포 키 도메인.
//!
//! 키 하나는 **로컬 키 쌍 하나 + 원격 등록 하나**다. 경로가 이미 어느 리포의
//! 무슨 키인지 말하므로, 키 자신은 역할을 갖지 않는다.
//!
//! 계정 자격과 결정적으로 다른 점은 **원격이 끼어든다**는 것이다. 계정 교체는
//! 디렉토리 맞바꾸기 하나로 원자적이지만, 키는 GitHub 에 등록하고 지우는 단계가
//! 사이에 있어 `rename` 으로 묶이지 않는다. 그래서 어디까지 갔는지를 상태로 남기고,
//! 중간에 죽어도 **아무것도 끊기지 않는 자리**에서만 멈추게 한다.

pub mod keyring;
pub mod repo;

use serde::{Deserialize, Serialize};

pub use keyring::Keyring;
pub use repo::RepoRef;

use crate::credential::secret::Secret;
use crate::port::ProgressSink;

/// 키가 어디까지 갔는가.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyState {
    /// 개인 키는 있는데 GitHub 등록이 끝나지 않았다. 아직 쓸 수 없다.
    Local,
    /// 쓸 수 있다.
    Registered,
    /// 재발급이 중간에 멈췄다. 새 키와 옛 키가 둘 다 등록돼 있어 끊기지는 않는다.
    Rotating,
}

/// 키가 놓이는 자리. 디렉토리 경로가 여기서 나온다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyRef {
    pub repo: RepoRef,
    /// 이 키가 무엇에 쓰이는가 — `coding` · `deploy` · `ci`.
    ///
    /// 경로가 이미 어느 리포인지 말하므로, 여기서 리포를 되풀이하면 아무것도
    /// 구분하지 못한다. 기계 이름도 쓰지 않는다 — 기계를 바꾸면 거짓이 된다.
    pub purpose: String,
}

impl KeyRef {
    pub fn new(repo: RepoRef, purpose: &str) -> Option<KeyRef> {
        let purpose = purpose.trim();
        if purpose.is_empty()
            || purpose == "."
            || purpose == ".."
            || !purpose
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return None;
        }
        Some(KeyRef {
            repo,
            purpose: purpose.to_string(),
        })
    }

    /// 화면과 기록에서 이 키를 가리키는 한 줄.
    pub fn slug(&self) -> String {
        format!("{}/{}", self.repo.slug(), self.purpose)
    }
}

/// 키 하나의 기록. `key.toml` 에 그대로 쓴다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployKey {
    /// 이 키가 무엇에 쓰이는가 — `coding` · `deploy` · `ci`.
    pub purpose: String,
    /// `owner/repo`.
    pub repo: String,
    /// 어느 마스터 계정으로 등록했나.
    pub account: String,
    pub write: bool,
    pub algorithm: String,
    pub fingerprint: String,
    pub comment: String,
    pub created_at: String,
    pub state: KeyState,
    /// GitHub 이 준 id. 지울 때 이 값이 있어야 한다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registered_at: Option<String>,
    /// 재발급 중 아직 GitHub 에 남아 있는 옛 키의 id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retiring_remote_id: Option<String>,
    /// 만료일 `YYYY-MM-DD` 또는 `never`. 사람이 적는다. 모르면 없다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
}

impl DeployKey {
    pub fn at(&self) -> Option<KeyRef> {
        KeyRef::new(RepoRef::parse(&self.repo)?, &self.purpose)
    }
}

/// 이 금고가 만들어 낸 키 쌍.
pub struct Material {
    pub public_key: String,
    pub fingerprint: String,
    pub algorithm: String,
}

/// GitHub 에 등록되어 있는 키 하나.
#[derive(Debug, Clone)]
pub struct RemoteKey {
    pub id: String,
    pub title: String,
    pub fingerprint: String,
    /// 계정 전체에 붙은 키면 `None`.
    pub repo: Option<String>,
    pub write: bool,
    pub created_at: Option<String>,
}

#[derive(Debug)]
pub enum KeyError {
    /// 그 자리에 이미 키가 있다.
    Taken(String),
    /// 그 자리에 키가 없다.
    Missing(String),
    /// 원격이 거절했거나 닿지 못했다.
    Remote(String),
    /// 로컬 파일을 다루지 못했다.
    Storage(String),
}

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyError::Taken(at) => write!(f, "{at} 자리에 이미 키가 있습니다"),
            KeyError::Missing(at) => write!(f, "{at} 자리에 키가 없습니다"),
            KeyError::Remote(detail) => write!(f, "GitHub: {detail}"),
            KeyError::Storage(detail) => write!(f, "{detail}"),
        }
    }
}

/// 원격에 키를 등록하고 지우는 곳.
pub trait KeyGateway: Send + Sync {
    fn register(
        &self,
        account: &str,
        at: &KeyRef,
        public_key: &str,
        write: bool,
        progress: &dyn ProgressSink,
    ) -> Result<String, KeyError>;

    fn unregister(
        &self,
        account: &str,
        repo: &RepoRef,
        remote_id: &str,
        progress: &dyn ProgressSink,
    ) -> Result<(), KeyError>;

    /// 그 리포에 등록된 배포 키 전부.
    fn deploy_keys(
        &self,
        account: &str,
        repo: &RepoRef,
        progress: &dyn ProgressSink,
    ) -> Result<Vec<RemoteKey>, KeyError>;

    /// 그 계정에 붙은 SSH 키 전부. 우리가 개인 키를 갖지 않은 것을 찾는 데 쓴다.
    fn account_keys(
        &self,
        account: &str,
        progress: &dyn ProgressSink,
    ) -> Result<Vec<RemoteKey>, KeyError>;
}

/// 개인 키와 기록이 실제로 놓이는 곳.
pub trait KeyVault: Send + Sync {
    fn exists(&self, at: &KeyRef) -> bool;

    /// 키 쌍을 만들어 **대기 자리**에 둔다. 제자리에는 아직 놓지 않는다.
    fn stage(&self, at: &KeyRef, comment: &str) -> Result<Material, KeyError>;

    /// 지금 제자리에 있는 키를 이력으로 물리고, 대기 중인 키를 제자리에 놓는다.
    fn place(&self, at: &KeyRef) -> Result<(), KeyError>;

    fn discard_staged(&self, at: &KeyRef);

    fn record(&self, key: &DeployKey) -> Result<(), KeyError>;
    fn load(&self, at: &KeyRef) -> Result<DeployKey, KeyError>;
    fn list(&self) -> Vec<Result<DeployKey, String>>;

    fn public_key(&self, at: &KeyRef) -> Result<String, KeyError>;
    fn staged_public_key(&self, at: &KeyRef) -> Result<String, KeyError>;
    fn private_key(&self, at: &KeyRef) -> Result<Secret, KeyError>;

    /// 실물을 보관소로 옮긴다. 지우지 않는다.
    fn archive(&self, at: &KeyRef, reason: &str) -> Result<(), KeyError>;

    /// 키가 놓인 자리를 옮긴다. 키 재료는 그대로다.
    fn move_to(&self, from: &KeyRef, to: &KeyRef) -> Result<(), KeyError>;
}
