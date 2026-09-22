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


use serde::{Deserialize, Serialize};

use crate::date;

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

    /// 이 provider 를 다루는 데 필요한 CLI 의 레지스트리 id.
    pub fn tool(&self) -> &'static str {
        match self {
            Provider::Github => "gh",
            Provider::Aws => "aws",
            Provider::Gcloud => "gcloud",
            Provider::Firebase => "firebase",
        }
    }
}

/// 자격이 만료에 얼마나 가까운가.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Expiry {
    /// 만료일이 적혀 있지 않다. 아직 확인하지 않았다는 뜻이다.
    Unset,
    /// 기한이 없는 자격임을 확인했다.
    ///
    /// 알릴 일은 없지만 좋은 상태도 아니다 — 무기한 자격은 유출돼도 스스로
    /// 만료되지 않으므로, 확인됐다는 사실만 기록하고 화면에서 구분해 보여준다.
    Never,
    Ok,
    /// 기한이 임박했다. 남은 일수를 들고 있다.
    Soon(i64),
    /// 이미 지났다. 지난 일수.
    Expired(i64),
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

    /// 만료까지 얼마나 남았는가. 오늘이 언제인지는 호출자가 안다.
    pub fn expiry_on(&self, today: &str) -> Expiry {
        let Some(raw) = self.expires.as_deref().map(str::trim) else {
            return Expiry::Unset;
        };
        if raw == NEVER {
            return Expiry::Never;
        }
        let Some(days) = date::days_between(today, raw) else {
            return Expiry::Unset;
        };
        if days < 0 {
            Expiry::Expired(-days)
        } else if days <= WARN_WITHIN_DAYS {
            Expiry::Soon(days)
        } else {
            Expiry::Ok
        }
    }

    /// 지금 사람에게 알려야 하는 상태인가.
    pub fn needs_attention_on(&self, today: &str) -> bool {
        matches!(self.expiry_on(today), Expiry::Soon(_) | Expiry::Expired(_))
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

/// 무엇 때문에 아카이브했는가.
///
/// 이 도구는 지우지 않고 물린다. 그래서 물린 이유가 남아야 나중에
/// "왜 이게 여기 있나" 를 답할 수 있다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArchiveReason {
    /// 자격을 새 것으로 갈아 끼웠다. 계정은 그대로 남는다.
    Replaced,
    /// 계정을 목록에서 내렸다. 계정 전체가 물러난다.
    Deleted,
}

impl ArchiveReason {
    pub fn id(&self) -> &'static str {
        match self {
            ArchiveReason::Replaced => "replaced",
            ArchiveReason::Deleted => "deleted",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ArchiveReason::Replaced => "교체",
            ArchiveReason::Deleted => "삭제",
        }
    }
}

/// 자격을 교체한 기록. 값은 담지 않는다.
///
/// 구 토큰은 GitHub 에서 재발급하는 순간 죽으므로 보관해도 복구에 쓸 수 없다.
/// 남길 값어치가 있는 건 "언제 무엇을 왜 바꿨나" 쪽이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replacement {
    pub replaced_at: String,
    pub reason: ArchiveReason,
    /// 교체를 부른 사정. `만료됨` 처럼 사람이 읽을 한 줄.
    #[serde(default)]
    pub detail: String,
    /// 교체 직전의 신원. 같은 계정으로 바꿨는지 나중에 확인할 수 있다.
    #[serde(default)]
    pub identity: String,
    #[serde(default)]
    pub expires: Option<String>,
    #[serde(default)]
    pub verified_at: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// 슬러그 규칙. 경로가 되므로 엄격하게 막는다.
pub fn validate_slug(slug: &str) -> Result<(), String> {
    if slug.is_empty() {
        return Err("이름이 비어 있습니다".into());
    }
    if slug.len() > 48 {
        return Err("이름이 너무 깁니다 (48자 이하)".into());
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("소문자·숫자·하이픈만 쓸 수 있습니다".into());
    }
    if slug.starts_with('-') || slug.ends_with('-') {
        return Err("하이픈으로 시작하거나 끝날 수 없습니다".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODAY: &str = "2026-09-22";

    #[test]
    fn a_slug_must_be_usable_as_a_directory_name() {
        assert!(validate_slug("tuk-prod").is_ok());
        assert!(validate_slug("").is_err());
        assert!(validate_slug("Tuk").is_err(), "대문자는 막는다");
        assert!(validate_slug("tuk prod").is_err(), "공백은 막는다");
        assert!(validate_slug("-tuk").is_err());
        assert!(validate_slug("tuk-").is_err());
        assert!(validate_slug(&"a".repeat(49)).is_err());
    }

    #[test]
    fn expiry_reads_the_date_the_way_the_list_shows_it() {
        let mut account = Account::new(Provider::Github, "octocat");
        assert!(matches!(account.expiry_on(TODAY), Expiry::Unset));

        account.expires = Some(NEVER.to_string());
        assert!(matches!(account.expiry_on(TODAY), Expiry::Never));

        account.expires = date::plus_days(TODAY, 3);
        assert!(matches!(account.expiry_on(TODAY), Expiry::Soon(3)), "곧 만료는 알려야 한다");
        assert!(account.needs_attention_on(TODAY));

        account.expires = date::plus_days(TODAY, 60);
        assert!(matches!(account.expiry_on(TODAY), Expiry::Ok));
        assert!(!account.needs_attention_on(TODAY));

        account.expires = date::plus_days(TODAY, -2);
        assert!(matches!(account.expiry_on(TODAY), Expiry::Expired(2)));
        assert!(account.needs_attention_on(TODAY));
    }

    #[test]
    fn an_unreadable_date_is_treated_as_unset_not_as_expired() {
        let mut account = Account::new(Provider::Aws, "admin");
        account.expires = Some("언젠가".to_string());
        assert!(matches!(account.expiry_on(TODAY), Expiry::Unset));
        assert!(!account.needs_attention_on(TODAY), "읽지 못한 날짜로 경고하지 않는다");
    }

    #[test]
    fn what_to_do_when_it_expires_depends_on_the_credential() {
        assert!(
            Account::new(Provider::Github, "a")
                .renewal_hint()
                .contains("새로 발급")
        );
        assert!(
            Account::new(Provider::Gcloud, "a")
                .renewal_hint()
                .contains("다시 로그인")
        );
    }

    #[test]
    fn an_archive_records_why_it_was_archived() {
        assert_eq!(ArchiveReason::Replaced.id(), "replaced");
        assert_eq!(ArchiveReason::Deleted.id(), "deleted");
        assert_ne!(ArchiveReason::Replaced.label(), ArchiveReason::Deleted.label());
    }
}
