//! GitHub — 토큰으로 붙고, 토큰이 가진 권한까지 함께 읽어 온다.

use std::io;

use secrets_core::account::Provider;
use secrets_core::identity::{AccountFacts, ObservedIdentity, Observation};

use super::{Browser, Field, LoginFlow, Method, Values, capture, header};
use crate::cli::{exec, tools};
use crate::vault::paths::env_for;

/// 토큰을 받아 적는다. 발급 주소도 함께 알려 준다.
pub(super) fn method() -> Method {
    Method {
        flow: LoginFlow::Credential,
        fields: github_fields(),
        browser: Some(Browser {
            label: "GitHub 에서 토큰 발급",
            url: github_token_url(),
        }),
        guidance: "GitHub 은 비밀번호로 CLI 인증을 받지 않습니다. 토큰을 발급해 붙여넣고 자격 확인을 누르면 계정 이름과 만료일을 읽어 옵니다.",
    }
}

/// 마스터 계정 토큰이 가져야 하는 권한과, 그것이 필요한 이유.
///
/// 발급 주소·안내 문구·자격 심사가 모두 이 목록 하나에서 만들어진다. 세 곳에 따로
/// 적으면 서로 어긋난다.
///
/// 삭제까지 하려면 `write:*` 가 아니라 `admin:*` 이어야 한다.
const GITHUB_SCOPES: &[(&str, &str)] = &[
    ("repo", "리포지토리 접근 — deploy key 등록·삭제"),
    ("admin:public_key", "계정 SSH 키 등록·삭제"),
    ("admin:gpg_key", "GPG 키 등록·삭제"),
    ("admin:ssh_signing_key", "SSH 서명 키 등록·삭제"),
];

/// 토큰 발급 페이지 주소. 필요한 범위를 미리 골라 준다.
pub(super) fn github_token_url() -> &'static str {
    static URL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    URL.get_or_init(|| {
        let scopes: Vec<&str> = GITHUB_SCOPES.iter().map(|(scope, _)| *scope).collect();
        format!(
            "https://github.com/settings/tokens/new?scopes={}&description=secrets-manager",
            scopes.join(",")
        )
    })
}

/// 폼이 받아 적을 칸. 안내가 발급 주소와 같은 목록에서 나온다.
pub(super) fn github_fields() -> &'static [Field] {
    static FIELDS: std::sync::OnceLock<Vec<Field>> = std::sync::OnceLock::new();
    FIELDS.get_or_init(|| {
        vec![Field {
            key: "token",
            label: "개인 액세스 토큰",
            secret: true,
            help: github_scope_help(),
            required: true,
        }]
    })
}

/// 폼에 적히는 안내. 발급 주소가 요청하는 것과 같아야 한다.
fn github_scope_help() -> &'static str {
    static HELP: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HELP.get_or_init(|| {
        let scopes: Vec<&str> = GITHUB_SCOPES.iter().map(|(scope, _)| *scope).collect();
        format!("{} 범위가 필요합니다", scopes.join(" · "))
    })
}

/// 이 토큰이 마스터 계정 노릇을 할 수 있는가.
///
/// 권한이 모자란 토큰을 마스터 계정으로 들이면 키 발급도 회전도 안 되는 껍데기가
/// 된다. AWS 에서 IAM 계정 정보를 못 읽는 신원을 막는 것과 같은 이유다.
fn missing_scopes(granted: &[String]) -> Vec<&'static str> {
    GITHUB_SCOPES
        .iter()
        .map(|(scope, _)| *scope)
        .filter(|scope| !granted.iter().any(|g| g == scope))
        .collect()
}


pub(super) fn connect_github<F>(
    home_dir: &std::path::Path,
    values: &Values,
    on_line: F,
) -> io::Result<exec::Outcome>
where
    F: Fn(exec::Stream, String) + Sync,
{
    let token = values.get("token").map(String::as_str).unwrap_or_default();
    let program = tools::find_in_path("gh")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "gh 를 찾을 수 없습니다"))?;

    // 토큰은 stdin 으로만 간다. argv 에 실으면 ps 로 읽힌다.
    exec::run_full(
        &program,
        &["auth", "login", "--with-token"],
        &env_for(Provider::Github, home_dir),
        Some(format!("{token}\n").as_bytes()),
        on_line,
    )
}

/// GitHub 은 신원과 함께 토큰의 권한·만료일까지 헤더로 알려 준다.
pub(super) fn probe_github(home_dir: &std::path::Path) -> io::Result<Observation> {
    // id 는 noreply 이메일을 만드는 데 쓴다. email 은 비공개면 비어서 온다.
    let (outcome, raw) = capture(
        Provider::Github,
        home_dir,
        "gh",
        &[
            "api",
            "user",
            "--jq",
            r#""\(.login)\t\(.id)\t\(.email // "")""#,
        ],
    )?;

    let fields: Vec<&str> = raw.trim().split('\t').collect();
    let login = fields.first().copied().unwrap_or_default().to_string();
    if !outcome.ok() || login.is_empty() {
        return Err(io::Error::other("GitHub 로그인 이름을 읽지 못했습니다"));
    }

    let (_, headers) = capture(Provider::Github, home_dir, "gh", &["api", "user", "-i"])?;

    // `2026-12-21 05:00:00 UTC` 형태로 온다. 날짜 부분만 쓴다.
    // 헤더가 없으면 기한 없는 토큰이다.
    let expires = match header(&headers, "github-authentication-token-expiration") {
        Some(raw) => raw
            .split_whitespace()
            .next()
            .filter(|d| secrets_core::time::parse(d).is_some())
            .map(str::to_string),
        None => Some(secrets_core::account::NEVER.to_string()),
    };

    let scopes: Vec<String> = header(&headers, "x-oauth-scopes")
        .map(|raw| {
            raw.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let missing = missing_scopes(&scopes);
    if !missing.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "이 토큰으로는 마스터 계정을 만들 수 없습니다. {} 범위가 없습니다",
                missing.join(" · ")
            ),
        ));
    }

    Ok(Observation {
        identity: ObservedIdentity::Github {
            login,
            user_id: fields.get(1).copied().unwrap_or_default().to_string(),
            public_email: fields.get(2).map(|e| e.trim().to_string()),
        },
        facts: AccountFacts {
            expires,
            scopes,
            ..AccountFacts::default()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    
    
    #[test]
    fn the_issue_url_asks_for_exactly_what_we_check_and_say() {
        let method = super::method();
        let url = method.browser.unwrap().url;

        let asked: Vec<&str> = url
            .split("scopes=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .split(',')
            .collect();
        let expected: Vec<&str> = GITHUB_SCOPES.iter().map(|(scope, _)| *scope).collect();

        // 요구하는 것과 심사하는 것이 정확히 같아야 한다. 더도 덜도 아니다.
        assert_eq!(asked, expected, "발급 주소가 심사 목록과 다르다");

        let help: Vec<&str> = method.fields[0]
            .help
            .trim_end_matches(" 범위가 필요합니다")
            .split(" · ")
            .collect();
        assert_eq!(help, expected, "안내가 심사 목록과 다르다");
    }

    #[test]
    fn a_token_without_the_needed_scopes_cannot_be_a_master_account() {
        let granted: Vec<String> = GITHUB_SCOPES
            .iter()
            .map(|(scope, _)| scope.to_string())
            .collect();
        assert!(missing_scopes(&granted).is_empty());

        // 읽기만 되는 토큰은 키를 발급할 수 없다.
        let read_only = vec!["repo".to_string(), "read:org".to_string()];
        let missing = missing_scopes(&read_only);
        assert!(missing.contains(&"admin:public_key"), "{missing:?}");
        assert!(missing.contains(&"admin:ssh_signing_key"), "{missing:?}");
        assert!(!missing.contains(&"repo"), "가진 범위를 없다고 하면 안 된다");

        // 더 넓은 범위를 가진 토큰은 막지 않는다.
        let mut generous = granted.clone();
        generous.push("admin:org".to_string());
        assert!(missing_scopes(&generous).is_empty());
    }

    #[test]
    fn github_asks_for_a_token_not_a_password() {
        let fields = super::method().fields;
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].key, "token");
        assert!(fields[0].secret);
    }

}
