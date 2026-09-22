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

use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{date, home};

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
            git_email: None,
            scopes: Vec::new(),
            expires: None,
        }
    }

    /// 만료까지 얼마나 남았는가.
    pub fn expiry(&self) -> Expiry {
        let Some(raw) = self.expires.as_deref().map(str::trim) else {
            return Expiry::Unset;
        };
        if raw == NEVER {
            return Expiry::Never;
        }
        let Some(days) = date::days_until(raw) else {
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
    pub fn needs_attention(&self) -> bool {
        matches!(self.expiry(), Expiry::Soon(_) | Expiry::Expired(_))
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

    /// 이 계정의 번들 디렉토리.
    pub fn dir(&self) -> PathBuf {
        dir_of(self.provider, &self.slug)
    }

    /// 이 계정 전용 CLI 설정 홈.
    pub fn cli_home(&self) -> PathBuf {
        self.dir().join("cli")
    }

    /// 이 계정으로 CLI 를 돌릴 때 덧씌울 환경변수.
    ///
    /// 격리의 실행 지점이다. 계정을 바꾼다는 건 이 값들을 바꾼다는 뜻이다.
    pub fn env(&self) -> Vec<(&'static str, String)> {
        env_for(self.provider, &self.cli_home())
    }

    /// 번들 디렉토리와 CLI 홈을 만들고 account.toml 을 쓴다.
    pub fn save(&self) -> io::Result<()> {
        let dir = self.dir();
        home::create_private(&dir)?;
        home::create_private(&self.cli_home())?;

        let text = toml::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let path = dir.join(FILE);
        std::fs::write(&path, text)?;
        home::restrict(&path)
    }
}

/// 주어진 CLI 홈을 가리키는 환경변수. 계정이 아직 없을 때도 쓴다.
pub fn env_for(provider: Provider, home_dir: &std::path::Path) -> Vec<(&'static str, String)> {
    let home = home_dir.display().to_string();
    match provider {
        Provider::Github => vec![("GH_CONFIG_DIR", home)],
        Provider::Gcloud => vec![("CLOUDSDK_CONFIG", home)],
        Provider::Firebase => vec![("XDG_CONFIG_HOME", home)],
        Provider::Aws => vec![
            (
                "AWS_CONFIG_FILE",
                home_dir.join("config").display().to_string(),
            ),
            (
                "AWS_SHARED_CREDENTIALS_FILE",
                home_dir.join("credentials").display().to_string(),
            ),
        ],
    }
}

const FILE: &str = "account.toml";
const HISTORY: &str = "history";

/// 자격을 교체한 기록. 값은 담지 않는다.
///
/// 구 토큰은 GitHub 에서 재발급하는 순간 죽으므로 보관해도 복구에 쓸 수 없다.
/// 남길 값어치가 있는 건 "언제 무엇을 왜 바꿨나" 쪽이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replacement {
    pub replaced_at: String,
    /// expired · rotated · reconnected
    pub reason: String,
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

impl Account {
    /// 지금 자격의 기록을 history 로 넘긴다. 실제 자격은 건드리지 않는다.
    ///
    /// 호출자가 새 자격을 붙이기 **직전에** 부른다.
    pub fn archive_credential(&self, reason: &str) -> io::Result<PathBuf> {
        let record = Replacement {
            replaced_at: date::now(),
            reason: reason.to_string(),
            identity: self.identity.name.clone(),
            expires: self.expires.clone(),
            verified_at: self.verification.as_ref().map(|v| v.checked_at.clone()),
            scopes: self.scopes.clone(),
        };

        let dir = unique(self.dir().join(HISTORY).join(date::today()));
        home::create_private(&dir)?;

        let text = toml::to_string_pretty(&record)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let path = dir.join("replaced.toml");
        std::fs::write(&path, text)?;
        home::restrict(&path)?;
        Ok(dir)
    }

    /// 지난 교체 기록. 최근 것이 앞에 온다.
    pub fn history(&self) -> Vec<Replacement> {
        let Ok(entries) = std::fs::read_dir(self.dir().join(HISTORY)) else {
            return Vec::new();
        };

        let mut dirs: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        dirs.reverse();

        dirs.iter()
            .filter_map(|d| std::fs::read_to_string(d.join("replaced.toml")).ok())
            .filter_map(|t| toml::from_str(&t).ok())
            .collect()
    }
}

/// 같은 날 두 번 교체할 수 있다. 덮어쓰지 않고 뒤에 번호를 붙인다.
fn unique(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    for n in 2..100 {
        let candidate = path.with_file_name(format!(
            "{}-{n}",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
        if !candidate.exists() {
            return candidate;
        }
    }
    path
}

pub fn dir_of(provider: Provider, slug: &str) -> PathBuf {
    home::root()
        .join(home::ACCOUNTS)
        .join(provider.id())
        .join(slug)
}

pub fn exists(provider: Provider, slug: &str) -> bool {
    dir_of(provider, slug).join(FILE).is_file()
}

pub fn load(provider: Provider, slug: &str) -> io::Result<Account> {
    let text = std::fs::read_to_string(dir_of(provider, slug).join(FILE))?;
    toml::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// 등록된 계정 전부. 읽을 수 없는 항목은 건너뛰지 않고 오류로 남긴다.
pub fn list() -> Vec<Result<Account, String>> {
    let mut found = Vec::new();
    let root = home::root().join(home::ACCOUNTS);

    for provider in Provider::ALL {
        let dir = root.join(provider.id());
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };

        let mut slugs: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        slugs.sort();

        for slug in slugs {
            if !exists(*provider, &slug) {
                continue;
            }
            found.push(
                load(*provider, &slug)
                    .map_err(|e| format!("{}/{slug} 를 읽을 수 없다: {e}", provider.id())),
            );
        }
    }
    found
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
    use crate::home::tests_support::with_temp_root;

    #[test]
    fn save_and_load_roundtrip() {
        with_temp_root(|_| {
            let mut account = Account::new(Provider::Github, "personal");
            account.display = "개인 계정".into();
            account.identity.name = "David-Lee-dev".into();
            account.save().unwrap();

            let loaded = load(Provider::Github, "personal").unwrap();
            assert_eq!(loaded.slug, "personal");
            assert_eq!(loaded.identity.name, "David-Lee-dev");
            assert!(account.cli_home().is_dir(), "CLI 홈이 만들어져야 한다");
        });
    }

    #[test]
    fn listing_is_sorted_and_grouped_by_provider() {
        with_temp_root(|_| {
            for (provider, slug) in [
                (Provider::Aws, "tuk"),
                (Provider::Github, "work"),
                (Provider::Github, "personal"),
            ] {
                Account::new(provider, slug).save().unwrap();
            }

            let names: Vec<String> = list()
                .into_iter()
                .map(|a| {
                    let a = a.unwrap();
                    format!("{}/{}", a.provider.id(), a.slug)
                })
                .collect();
            assert_eq!(names, ["github/personal", "github/work", "aws/tuk"]);
        });
    }

    #[test]
    fn env_points_at_the_accounts_own_cli_home() {
        with_temp_root(|_| {
            let account = Account::new(Provider::Github, "personal");
            let env = account.env();
            assert_eq!(env.len(), 1);
            assert_eq!(env[0].0, "GH_CONFIG_DIR");
            assert!(env[0].1.ends_with("accounts/github/personal/cli"));
        });
    }

    #[test]
    fn archived_record_carries_the_facts_but_no_secret() {
        with_temp_root(|_| {
            let mut account = Account::new(Provider::Github, "personal");
            account.identity.name = "David-Lee-dev".into();
            account.expires = Some("2026-10-22".into());
            account.scopes = vec!["repo".into(), "admin:org".into()];
            account.verification = Some(Verification {
                checked_at: "2026-09-22T00:00:00Z".into(),
                ok: true,
                detail: String::new(),
            });
            account.save().unwrap();

            let dir = account.archive_credential("expired").unwrap();
            let text = std::fs::read_to_string(dir.join("replaced.toml")).unwrap();

            assert!(text.contains("David-Lee-dev"));
            assert!(text.contains("2026-10-22"));
            assert!(text.contains("expired"));
            // 값은 기록하지 않는다. 죽은 비밀을 디스크에 남기지 않기 위해서다.
            assert!(!text.contains("ghp_"), "{text}");

            let history = account.history();
            assert_eq!(history.len(), 1);
            assert_eq!(history[0].identity, "David-Lee-dev");
            assert_eq!(history[0].scopes, ["repo", "admin:org"]);
        });
    }

    #[test]
    fn history_is_newest_first_and_survives_same_day_replacements() {
        with_temp_root(|_| {
            let mut account = Account::new(Provider::Github, "personal");
            account.save().unwrap();

            account.identity.name = "first".into();
            account.archive_credential("rotated").unwrap();
            account.identity.name = "second".into();
            account.archive_credential("expired").unwrap();

            let history = account.history();
            assert_eq!(history.len(), 2, "같은 날 두 번 바꿔도 덮어쓰지 않는다");
            assert_eq!(history[0].identity, "second", "최근 것이 앞에 온다");
        });
    }

    #[test]
    fn slug_rules() {
        assert!(validate_slug("tuk-prod").is_ok());
        assert!(validate_slug("").is_err());
        assert!(validate_slug("Tuk").is_err());
        assert!(validate_slug("tuk_prod").is_err());
        assert!(validate_slug("-tuk").is_err());
        assert!(validate_slug("../etc").is_err());
    }
}
