//! 최소 권한 IAM 사용자와 그 액세스 키.
//!
//! IAM 하나는 권한 하나다. 금고가 만들고 키를 발급한다. `.env` 에 넣고 빼는 일은
//! 사람이 하고, 금고는 **어디에 넣었는지 기록만** 한다.
//!
//! 키를 바꾸는 기능은 없다. 새 IAM 을 만들고 옛 것은 사람이 치운다 — 기록에서 빠진
//! 소비처가 하나라도 있으면 자동 교체가 그곳을 끊는다. 대신 옛 IAM 은 **키가 한 달
//! 넘게 쓰이지 않았을 때만** 지울 수 있다. 기록이 틀려도 아직 쓰이는 키는 지워지지 않는다.
//!
//! 화면에는 IAM 이 있거나 없거나만 보인다. 만들다 실패하면 되돌려서 반쯤 된 IAM 이
//! 남지 않게 한다.

pub mod issuer;
pub mod naming;
pub mod policy;

use serde::{Deserialize, Serialize};

pub use issuer::{Draft, IDLE_DAYS, Issuer};
pub use naming::{Env, IamName, Naming, Sibling};
pub use policy::{Policy, PolicyError, Probe};

use crate::credential::secret::Secret;
use crate::port::ProgressSink;

/// 이 맥. 소비처의 호스트 자리에 쓴다.
pub const LOCAL_HOST: &str = "local";

/// IAM 이 놓이는 자리. AWS 계정 ID 가 경로의 첫 단계다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IamRef {
    pub account: String,
    pub name: String,
}

impl IamRef {
    pub fn slug(&self) -> String {
        format!("{}/{}", self.account, self.name)
    }
}

/// 키가 들어간 곳 하나.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Consumer {
    /// `~/.ssh/config` 의 호스트 이름. 이 맥이면 [`LOCAL_HOST`].
    pub host: String,
    /// 그 호스트에서의 `.env` 파일 경로. `~` 로 시작할 수 있다.
    pub file: String,
    pub id_variable: String,
    pub secret_variable: String,
    #[serde(default)]
    pub recorded_at: String,
}

impl Consumer {
    /// 같은 파일의 같은 변수인가. 한 자리에는 키 하나만 들어간다.
    pub fn same_place(&self, other: &Consumer) -> bool {
        self.host == other.host && self.file == other.file && self.id_variable == other.id_variable
    }

    pub fn slug(&self) -> String {
        format!("{}:{} {}", self.host, self.file, self.id_variable)
    }

    /// 키 ID 변수에서 시크릿 변수를 정한다. `…_ACCESS_KEY_ID` → `…_SECRET_ACCESS_KEY`.
    ///
    /// SDK 들이 읽는 기본 이름(`AWS_ACCESS_KEY_ID` · `AWS_SECRET_ACCESS_KEY`)이 이 규칙에
    /// 들어맞는다. 규칙 밖의 이름은 받지 않는다 — 짝이 어긋나면 앱이 엉뚱한 시크릿을 읽는다.
    pub fn secret_variable_for(id_variable: &str) -> Option<String> {
        let id = id_variable.trim();
        let valid = !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
        if !valid {
            return None;
        }
        let stem = id.strip_suffix("ACCESS_KEY_ID")?;
        Some(format!("{stem}SECRET_ACCESS_KEY"))
    }
}

/// IAM 하나의 기록. `iam.toml` 에 그대로 쓴다. 정책 원문과 시크릿은 따로 둔다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IamUser {
    pub name: String,
    pub app: String,
    pub env: String,
    pub perm: String,
    #[serde(default)]
    pub purpose: String,
    /// AWS 계정 ID.
    pub account: String,
    /// 이 IAM 을 만든 마스터 계정. AWS 에 물을 때 이 계정으로 나간다.
    pub master: String,
    pub key_id: String,
    pub issued_at: String,
    pub created_at: String,
    #[serde(default)]
    pub consumers: Vec<Consumer>,
    /// 마지막으로 AWS 에 사용 기록을 물은 결과. 묻기 전이면 없다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<UseCheck>,
    /// 이 날부터 지울 수 있다. `YYYY-MM-DD`.
    ///
    /// 하한이다. 마지막 사용은 앞으로만 움직이므로, 키가 다시 쓰이면 늦춰질 수는
    /// 있어도 당겨지지는 않는다. 그래서 이 날 전에는 AWS 에 묻지 않고도 막을 수 있다.
    #[serde(default)]
    pub deletable_from: String,
}

/// AWS 에 사용 기록을 물은 한 번.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UseCheck {
    pub checked_at: String,
    /// 한 번도 쓰이지 않았으면 없다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<LastUse>,
}

impl IamUser {
    pub fn at(&self) -> IamRef {
        IamRef {
            account: self.account.clone(),
            name: self.name.clone(),
        }
    }
}

/// 키가 마지막으로 쓰인 때.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastUse {
    pub at: String,
    pub service: String,
    pub region: String,
}

#[derive(Debug)]
pub enum IamError {
    /// 받은 값이 규칙에 맞지 않는다.
    Invalid(String),
    Taken(String),
    Missing(String),
    /// AWS 가 거절했거나 닿지 못했다.
    Remote(String),
    /// 만든 정책이 적힌 대로 동작하지 않는다.
    Probe(String),
    /// 키가 최근에 쓰였다. 어딘가에서 아직 쓰는 중일 수 있다.
    Recent {
        idle_days: i64,
        last: String,
        from: String,
    },
    Storage(String),
}

impl std::fmt::Display for IamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IamError::Invalid(detail) => write!(f, "{detail}"),
            IamError::Taken(at) => write!(f, "{at} 은(는) 이미 있습니다"),
            IamError::Missing(at) => write!(f, "{at} 이(가) 없습니다"),
            IamError::Remote(detail) => write!(f, "AWS: {detail}"),
            IamError::Probe(detail) => write!(f, "정책 확인: {detail}"),
            IamError::Recent {
                idle_days,
                last,
                from,
            } => write!(
                f,
                "키가 {last} 에 쓰였습니다 ({idle_days}일 전). {IDLE_DAYS}일이 넘게 쓰이지 않아야 지울 수 있습니다 — {from} 부터"
            ),
            IamError::Storage(detail) => write!(f, "{detail}"),
        }
    }
}

/// AWS 의 IAM 에 하는 일. 마스터 계정으로 나간다.
pub trait IamGateway: Send + Sync {
    fn account_id(&self, master: &str, progress: &dyn ProgressSink) -> Result<String, IamError>;

    fn create_user(&self, master: &str, name: &str, progress: &dyn ProgressSink)
    -> Result<(), IamError>;

    /// 인라인 정책을 붙인다. 정책 이름은 사용자 이름과 같다.
    fn put_policy(
        &self,
        master: &str,
        name: &str,
        policy: &str,
        progress: &dyn ProgressSink,
    ) -> Result<(), IamError>;

    /// 그 동작이 그 대상에 허용되는가. AWS 의 정책 시뮬레이터에 묻는다.
    fn allows(
        &self,
        master: &str,
        at: &IamRef,
        probe: &Probe,
        progress: &dyn ProgressSink,
    ) -> Result<bool, IamError>;

    /// 새 액세스 키. 시크릿은 이때 한 번만 받을 수 있다.
    fn issue_key(
        &self,
        master: &str,
        name: &str,
        progress: &dyn ProgressSink,
    ) -> Result<(String, Secret), IamError>;

    /// 그 키로 AWS 에 들어가 본다. 돌려주는 것은 그 키의 주인 ARN.
    fn identify(
        &self,
        key_id: &str,
        secret: &Secret,
        progress: &dyn ProgressSink,
    ) -> Result<String, IamError>;

    /// 사용자를 지운다. 달린 키와 인라인 정책을 먼저 걷어낸다 — AWS 는 그것들이
    /// 남은 사용자를 지우지 않는다.
    fn delete_user(&self, master: &str, name: &str, progress: &dyn ProgressSink)
    -> Result<(), IamError>;

    fn last_used(
        &self,
        master: &str,
        key_id: &str,
        progress: &dyn ProgressSink,
    ) -> Result<Option<LastUse>, IamError>;
}

/// 기록 · 정책 원문 · 시크릿이 놓이는 곳.
pub trait IamVault: Send + Sync {
    fn exists(&self, at: &IamRef) -> bool;
    /// 그 AWS 계정에 있는 IAM 이름 전부. 이름이 겹치는지 볼 때 쓴다.
    fn names(&self, account: &str) -> Vec<String>;
    fn list(&self) -> Vec<Result<IamUser, String>>;
    fn load(&self, at: &IamRef) -> Result<IamUser, IamError>;
    fn record(&self, user: &IamUser) -> Result<(), IamError>;

    /// 새 IAM 의 기록 · 정책 · 시크릿을 처음 놓는다.
    fn keep(&self, user: &IamUser, policy: &str, secret: &Secret) -> Result<(), IamError>;
    fn policy(&self, at: &IamRef) -> Result<String, IamError>;
    fn secret(&self, at: &IamRef) -> Result<Secret, IamError>;

    /// 실물을 보관소로 옮긴다. 지우지 않는다.
    fn archive(&self, at: &IamRef, reason: &str) -> Result<(), IamError>;
    /// 만들다 만 것을 치운다. 보관할 가치가 없다 — AWS 에도 남지 않았다.
    fn discard(&self, at: &IamRef);
}

#[cfg(test)]
mod tests {
    use super::*;

    mod secret_variable_for {
        use super::*;

        #[test]
        fn pairs_the_sdk_default_names() {
            assert_eq!(
                Consumer::secret_variable_for("AWS_ACCESS_KEY_ID").as_deref(),
                Some("AWS_SECRET_ACCESS_KEY")
            );
        }

        #[test]
        fn pairs_a_permission_scoped_name() {
            assert_eq!(
                Consumer::secret_variable_for("AWS_S3_ACCESS_KEY_ID").as_deref(),
                Some("AWS_S3_SECRET_ACCESS_KEY")
            );
        }

        #[test]
        fn refuses_names_outside_the_rule_or_with_shell_characters() {
            assert_eq!(Consumer::secret_variable_for("S3_KEY"), None);
            assert_eq!(Consumer::secret_variable_for("aws_access_key_id"), None);
            assert_eq!(Consumer::secret_variable_for("X;ACCESS_KEY_ID"), None);
        }
    }
}
