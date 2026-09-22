//! 계정 연결 — provider 마다 무엇을 입력받아 어떻게 로그인시키는가.
//!
//! provider 별로 받을 수 있는 값이 다르다. 아무거나 id/pw 로 뭉뚱그리면
//! 쓰지도 못할 값을 보관하게 된다.
//!
//! - GitHub  : 개인 액세스 토큰. 비밀번호 인증은 2021 년에 폐지됐다.
//! - AWS     : 액세스 키 ID + 시크릿. 유일하게 폼이 그대로 맞는 provider.
//! - GCP·Firebase : 브라우저 OAuth 만 가능하다. 입력받을 값이 없다.
//!
//! 비밀값은 언제나 stdin 이나 파일로만 넘어간다. 명령행 인자로 넘기지 않는다.

use std::io;

pub mod aws;
pub mod browser;
pub mod github;
pub mod form;
pub mod google;

use aws::{connect_aws, probe_aws};
use github::{connect_github, probe_github};
use google::{probe_firebase, probe_gcloud};

pub use browser::{Challenge, browser_begin_in, browser_complete_in};
pub use form::{Browser, Field, Method, Values};

use secrets_core::account::{Account, Provider};

use crate::vault::paths::{self, env_for};
use secrets_core::identity::Observation;
use crate::cli::{exec, tools};
use crate::vault;

/// 이 provider 를 연결하려면 무엇을 받아 적어야 하는가.
///
/// 무엇이 필요한지는 provider 자신이 안다. 여기서는 물어 볼 상대만 고른다.
pub fn method(provider: Provider) -> Method {
    match provider {
        Provider::Github => github::method(),
        Provider::Aws => aws::method(),
        Provider::Gcloud | Provider::Firebase => google::method(provider),
    }
}

/// 폼이 요구하는 값이 다 왔는지 확인한다.
pub fn validate(provider: Provider, values: &Values) -> Result<(), String> {
    for field in method(provider).fields {
        if field.required
            && values
                .get(field.key)
                .map(|v| v.trim().is_empty())
                .unwrap_or(true)
        {
            return Err(format!("{} 을(를) 입력하세요", field.label));
        }
    }
    Ok(())
}

/// 이 provider 를 다루는 CLI 의 레지스트리 id.
///
/// 어떤 명령줄 도구로 그 provider 를 다루는지는 core 가 알 일이 아니다.
pub fn tool_for(provider: Provider) -> &'static str {
    match provider {
        Provider::Github => "gh",
        Provider::Aws => "aws",
        Provider::Gcloud => "gcloud",
        Provider::Firebase => "firebase",
    }
}

/// 입력값으로 실제 로그인을 수행한다.
///
/// 로그인은 계정 전용 CLI 홈 안에서만 일어난다. 기존 로그인은 건드리지 않는다.
pub fn connect<F>(account: &Account, values: &Values, on_line: F) -> io::Result<exec::Outcome>
where
    F: Fn(exec::Stream, String) + Sync,
{
    connect_into(account.provider, &paths::cli_home(account), values, on_line)
}

/// 지정한 CLI 홈에 로그인한다.
///
/// 계정을 만들기 전에 자격을 확인해 보려면 임시 홈이 필요하므로, 경로를 받는다.
pub fn connect_into<F>(
    provider: Provider,
    home_dir: &std::path::Path,
    values: &Values,
    on_line: F,
) -> io::Result<exec::Outcome>
where
    F: Fn(exec::Stream, String) + Sync,
{
    // 홈이 없으면 CLI 가 엉뚱한 곳에 쓴다. 먼저 보장한다.
    vault::create_private(home_dir)?;

    match provider {
        Provider::Github => connect_github(home_dir, values, on_line),
        Provider::Aws => connect_aws(home_dir, values),
        Provider::Gcloud | Provider::Firebase => browser_login(provider, home_dir, on_line),
    }
}

/// 브라우저를 열어 로그인시키고 끝날 때까지 기다린다.
///
/// CLI 가 브라우저를 띄우고 localhost 로 결과를 받아 스스로 완료하므로,
/// 우리가 중간에 코드를 받아 넘길 필요가 없다. 사람이 브라우저에서 끝내는 동안
/// 이 호출은 막혀 있으므로 호출자는 별도 스레드에서 불러야 한다.
fn browser_login<F>(
    provider: Provider,
    home_dir: &std::path::Path,
    on_line: F,
) -> io::Result<exec::Outcome>
where
    F: Fn(exec::Stream, String) + Sync,
{
    let (tool, args): (_, &[&str]) = match provider {
        Provider::Gcloud => ("gcloud", &["auth", "login", "--brief"]),
        // --no-localhost 를 명시한다. 붙이지 않아도 출력이 TTY 가 아니면 같은
        // 흐름으로 빠지지만, 그러면 인증 페이지가 안내하는 명령과 우리가 실제로
        // 실행한 명령이 달라 사용자가 대조할 수 없다.
        Provider::Firebase => ("firebase", &["login", "--no-localhost"]),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "브라우저 로그인 대상이 아닙니다",
            ));
        }
    };

    let program = tools::find_in_path(tool).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("{tool} 를 찾을 수 없습니다"),
        )
    })?;

    exec::run_env(&program, args, &env_for(provider, home_dir), on_line)
}

/// 주어진 홈에 입력값으로 로그인하고 신원을 읽는다.
///
/// 로그인 결과는 이 홈에 남는다. 호출자가 그 홈을 계정 홈으로 그대로 옮기므로
/// 같은 자격으로 두 번 로그인할 일이 없다.
pub fn probe_in<F>(
    provider: Provider,
    home_dir: &std::path::Path,
    values: &Values,
    on_line: F,
) -> io::Result<Observation>
where
    F: Fn(exec::Stream, String) + Sync,
{
    validate(provider, values).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let outcome = connect_into(provider, home_dir, values, on_line)?;
    if !outcome.ok() {
        return Err(io::Error::other("자격으로 로그인하지 못했습니다"));
    }
    probe_home(provider, home_dir)
}

/// 주어진 홈에서 브라우저로 로그인시키고 신원을 읽는다. 한 번에 끝나는 provider 용.
///
/// 로그인 결과는 이 홈에 남는다. 호출자가 계정 홈으로 옮기므로 브라우저를 두 번
/// 띄우지 않는다.
pub fn browser_probe_in<F>(
    provider: Provider,
    home_dir: &std::path::Path,
    on_line: F,
) -> io::Result<Observation>
where
    F: Fn(exec::Stream, String) + Sync,
{
    vault::create_private(home_dir)?;

    let outcome = browser_login(provider, home_dir, on_line)?;
    if !outcome.ok() {
        return Err(io::Error::other("브라우저 로그인이 완료되지 않았습니다"));
    }
    probe_home(provider, home_dir)
}

/// 이미 로그인된 홈에서 신원을 읽는다.
///
/// 브라우저 로그인은 확인 단계가 따로 없다 — 로그인 자체가 확인이므로,
/// 로그인이 끝난 뒤 그 홈을 그대로 읽는다.
pub fn probe_home(provider: Provider, home_dir: &std::path::Path) -> io::Result<Observation> {
    probe_home_logging(provider, home_dir, |_, _| {})
}

/// 신원을 읽으면서 실행 명령을 터미널에도 보여 준다.
pub fn probe_home_logging<F>(
    provider: Provider,
    home_dir: &std::path::Path,
    on_line: F,
) -> io::Result<Observation>
where
    F: Fn(exec::Stream, String) + Sync,
{
    // 어떤 명령이 나갔는지는 보이게 하되, 출력 해석은 provider 별 함수에 맡긴다.
    on_line(exec::Stream::Stdout, format!("{} 신원 확인", provider.id()));

    match provider {
        Provider::Github => probe_github(home_dir),
        Provider::Aws => probe_aws(home_dir),
        Provider::Gcloud => probe_gcloud(home_dir),
        Provider::Firebase => probe_firebase(home_dir),
    }
}

/// 헤더 이름으로 값을 찾는다. 이름의 대소문자는 서버마다 다르다.
fn header<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let wanted = format!("{}:", name.to_ascii_lowercase());
    text.lines()
        .find(|line| line.to_ascii_lowercase().starts_with(&wanted))
        .and_then(|line| line.split_once(':'))
        .map(|(_, value)| value.trim())
}

/// 지정한 홈에서 CLI 를 돌리고 출력을 통째로 받는다.
fn capture(
    provider: Provider,
    home_dir: &std::path::Path,
    tool: &str,
    args: &[&str],
) -> io::Result<(exec::Outcome, String)> {
    let program = tools::find_in_path(tool).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("{tool} 를 찾을 수 없습니다"),
        )
    })?;

    let buffer = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = buffer.clone();

    let outcome = exec::run_env(
        &program,
        args,
        &env_for(provider, home_dir),
        move |stream, line| {
            // 파서에는 stdout 만 준다. stderr 는 CLI 가 내는 경고·진단이고, 섞이면
            // 그 문장이 신원으로 읽힌다 — 실제로 firebase 의 안내 전문이 계정
            // 이름이 된 적이 있다.
            if stream != exec::Stream::Stdout {
                return;
            }
            if let Ok(mut buf) = sink.lock() {
                buf.push_str(&line);
                buf.push('\n');
            }
        },
    )?;

    let text = buffer.lock().map(|b| b.clone()).unwrap_or_default();
    Ok((outcome, text))
}


#[cfg(test)]
mod tests {
    use super::*;
    
    

    fn values(pairs: &[(&str, &str)]) -> Values {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn header_lookup_is_case_insensitive() {
        let raw = "HTTP/2.0 200 OK\nX-Oauth-Scopes: repo, read:org\nDate: x\n";
        assert_eq!(header(raw, "x-oauth-scopes"), Some("repo, read:org"));
        assert_eq!(header(raw, "X-OAUTH-SCOPES"), Some("repo, read:org"));
        assert_eq!(header(raw, "missing"), None);
    }

    #[test]
    fn browser_only_providers_have_no_fields() {
        for provider in [Provider::Gcloud, Provider::Firebase] {
            let method = method(provider);
            assert!(method.fields.is_empty(), "{provider:?}");
            assert!(method.browser_login, "{provider:?} 는 브라우저로 연결한다");
        }
        // 값을 받아 적는 provider 는 브라우저 로그인이 아니다.
        for provider in [Provider::Github, Provider::Aws] {
            assert!(!method(provider).browser_login, "{provider:?}");
        }
    }

    #[test]
    fn only_firebase_needs_a_code_pasted_back() {
        assert!(method(Provider::Firebase).browser_code);
        // gcloud 는 localhost 로 결과를 받아 스스로 끝낸다.
        assert!(!method(Provider::Gcloud).browser_code);
        assert!(!method(Provider::Github).browser_code);
    }

    #[test]
    fn validation_reports_the_missing_field_by_label() {
        let err = validate(Provider::Aws, &values(&[("access_key_id", "AKIA")])).unwrap_err();
        assert!(err.contains("Secret Access Key"), "{err}");

        assert!(
            validate(
                Provider::Aws,
                &values(&[("access_key_id", "AKIA"), ("secret_access_key", "x"),]),
            )
            .is_ok(),
            "리전은 선택 항목이다"
        );
    }

}
