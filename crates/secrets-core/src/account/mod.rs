//! 계정 레지스트리.
//!
//! 계정은 자격을 발급하는 주체다. 키는 계정이 만들어 내므로 계정이 먼저 있어야 한다.
//!
//! 계정마다 CLI 설정 홈을 따로 준다. 이게 계정별 격리의 실체다 —
//! `gh` 를 A 계정으로 쓰는 것과 B 계정으로 쓰는 것의 차이는 `GH_CONFIG_DIR` 이
//! 어느 디렉토리를 가리키느냐뿐이다.
//!
//! ```text
//! ~/.secrets/accounts/github/personal/
//!   account.toml   이 계정이 무엇인가
//!   cli/           이 계정 전용 CLI 설정 홈
//! ```


mod archive;
mod expiry;
mod naming;

pub use archive::{ArchiveReason, Replacement};
pub use expiry::Expiry;
pub use naming::validate_slug;

use serde::{Deserialize, Serialize};


/// 만료가 이만큼 남으면 상시로 알린다.
pub const WARN_WITHIN_DAYS: i64 = 7;

/// `expires` 에 이 값이 적히면 기한이 없는 자격이라는 뜻이다.
pub const NEVER: &str = "never";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Github,
    Aws,
    Gcloud,
    Firebase,
}

impl Provider {
    pub const ALL: &'static [Provider] = &[
        Provider::Github,
        Provider::Aws,
        Provider::Gcloud,
        Provider::Firebase,
    ];

    /// 디렉토리 이름이자 외부 표기.
    pub fn id(&self) -> &'static str {
        match self {
            Provider::Github => "github",
            Provider::Aws => "aws",
            Provider::Gcloud => "gcloud",
            Provider::Firebase => "firebase",
        }
    }

    pub fn parse(text: &str) -> Option<Provider> {
        Provider::ALL.iter().copied().find(|p| p.id() == text)
    }

}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    /// iam-user, sso-role, oauth 등. 검증으로 채워진다.
    #[serde(default)]
    pub kind: String,
    /// 로그인 이름 · 이메일 · ARN 등 이 계정에서 나를 가리키는 것.
    #[serde(default)]
    pub name: String,
}

/// 마지막 검증 결과. 선언이 아니라 실제로 확인된 사실만 여기 들어간다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verification {
    /// ISO 8601 날짜시각.
    pub checked_at: String,
    pub ok: bool,
    /// 판정 근거. 실패했을 때 원인을 남긴다.
    #[serde(default)]
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    /// 디렉토리 이름과 같다. 불변.
    pub slug: String,
    pub provider: Provider,
    /// 사람이 읽을 이름.
    #[serde(default)]
    pub display: String,
    #[serde(default)]
    pub note: String,
    #[serde(default = "Identity::empty")]
    pub identity: Identity,
    #[serde(default)]
    pub verification: Option<Verification>,
    /// AWS 계정 번호. 같은 계정에 속한 신원끼리 묶어 볼 수 있게 남긴다.
    #[serde(default)]
    pub aws_account_id: Option<String>,
    /// root 에 액세스 키가 있는가. AWS 는 만들지 말라고 권고한다.
    #[serde(default)]
    pub root_keys_present: Option<bool>,
    /// root 에 MFA 가 걸려 있는가.
    #[serde(default)]
    pub root_mfa: Option<bool>,
    /// 이 계정으로 커밋할 때 쓸 이메일.
    ///
    /// gh 는 커밋 신원을 건드리지 않는다. 계정만 바꾸고 이걸 놔두면 커밋이
    /// 이전 계정 이메일로 나가고, GitHub 에서 다른 사람 커밋으로 잡힌다.
    #[serde(default)]
    pub git_email: Option<String>,
    /// 이 자격이 가진 권한. GitHub 토큰의 scope 등.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// 이 계정에 쓰는 자격의 만료일 (`YYYY-MM-DD`).
    ///
    /// GitHub 토큰처럼 기한이 있는 자격에만 의미가 있다. 만료되면 이 계정으로
    /// 하는 모든 작업이 조용히 실패하므로, 미리 알리려고 사람이 적어 둔다.
    #[serde(default)]
    pub expires: Option<String>,
}

impl Identity {
    fn empty() -> Identity {
        Identity {
            kind: String::new(),
            name: String::new(),
        }
    }
}

impl Account {
    pub fn new(provider: Provider, slug: &str) -> Account {
        Account {
            slug: slug.to_string(),
            provider,
            display: String::new(),
            note: String::new(),
            identity: Identity::empty(),
            verification: None,
            aws_account_id: None,
            root_keys_present: None,
            root_mfa: None,
            git_email: None,
            scopes: Vec::new(),
            expires: None,
        }
    }
    /// 만료됐을 때 무엇을 해야 하는가.
    ///
    /// 자격 종류마다 다르다. SSH 키와 토큰은 값이 바뀌므로 새로 만들어야 하고,
    /// GPG 키만 기한 연장이 된다.
    pub fn renewal_hint(&self) -> &'static str {
        match self.provider {
            Provider::Github => "토큰은 연장할 수 없습니다. 새로 발급해 다시 연결하세요.",
            Provider::Aws => "액세스 키는 새로 발급하고 구 키를 비활성화하세요.",
            Provider::Gcloud | Provider::Firebase => "다시 로그인하세요.",
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    

    const TODAY: &str = "2026-09-22";

    #[test]
    fn an_unreadable_date_is_treated_as_unset_not_as_expired() {
        let mut account = Account::new(Provider::Aws, "admin");
        account.expires = Some("언젠가".to_string());
        assert!(matches!(account.expiry_on(TODAY), Expiry::Unset));
        assert!(!account.needs_attention_on(TODAY), "읽지 못한 날짜로 경고하지 않는다");
    }

}
