//! 브라우저로 끝내는 로그인.
//!
//! gcloud 는 localhost 로 결과를 받아 스스로 끝낸다. firebase 는 출력이 TTY 가
//! 아니면 주소와 코드 입력을 요구하는 흐름으로 빠지므로 두 단계로 나눠야 한다.

use std::io;

use secrets_core::account::Provider;
use secrets_core::identity::Observation;

use crate::cli::{exec, tools};
use crate::vault;
use crate::vault::paths::env_for;

use super::probe_home;

/// 코드를 받아 와야 끝나는 로그인의 첫 단계.
#[derive(Debug, Clone)]
pub struct Challenge {
    /// 사람이 열어야 할 주소.
    pub url: String,
    /// 브라우저 페이지에서 대조할 세션 번호.
    ///
    /// 탭이 여러 개 떠 있으면 다른 세션의 코드를 붙여넣기 쉽다. 그러면 서버가
    /// 코드를 거부하는데 이유가 드러나지 않는다. 대조할 수 있게 보여 준다.
    pub session: String,
    /// CLI 가 알려 준 안내 전문.
    pub note: String,
}

/// 주어진 홈에서 로그인을 시작해 인증 주소를 받아 온다.
///
/// 두 번째 단계가 같은 홈을 써야 세션이 이어진다.
pub fn browser_begin_in<F>(
    provider: Provider,
    stage: &std::path::Path,
    on_line: F,
) -> io::Result<Challenge>
where
    F: Fn(exec::Stream, String) + Sync,
{
    // 이 흐름은 firebase 전용이다. 다른 provider 로 부르면 firebase 를 그 provider
    // 의 격리 설정으로 돌리게 되고, 그 설정에는 XDG_CONFIG_HOME 이 없어 사용자의
    // 실제 firebase 로그인에 닿는다.
    only_firebase(provider)?;
    vault::create_private(stage)?;

    let program = tools::find_in_path("firebase")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "firebase 를 찾을 수 없습니다"))?;

    let buffer = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = buffer.clone();

    exec::run_env(
        &program,
        &["login", "--no-localhost"],
        &env_for(provider, stage),
        move |stream, line| {
            if let Ok(mut buf) = sink.lock() {
                buf.push_str(&line);
                buf.push('\n');
            }
            on_line(stream, line);
        },
    )?;

    let note = buffer.lock().map(|b| b.clone()).unwrap_or_default();
    let url = first_url(&note).ok_or_else(|| io::Error::other("인증 주소를 찾지 못했습니다"))?;

    let session = session_id(&note, &url);
    Ok(Challenge { url, session, note })
}

/// 코드를 되돌려 넣는 두 단계 로그인은 firebase 만 한다.
fn only_firebase(provider: Provider) -> io::Result<()> {
    if provider == Provider::Firebase {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{}은(는) 코드 입력 방식의 로그인을 지원하지 않습니다.", provider.id()),
    ))
}

/// 안내 전문에서 세션 번호를 찾는다.
///
/// CLI 가 `session ID:` 다음 줄에 찍어 주고, 그 값은 주소의 session 앞부분이다.
/// 출력 형식이 바뀌어도 주소에서 뽑을 수 있게 두 갈래로 둔다.
fn session_id(note: &str, url: &str) -> String {
    let lines: Vec<&str> = note.lines().map(str::trim).collect();
    if let Some(i) = lines.iter().position(|l| l.contains("session ID"))
        && let Some(found) = lines[i + 1..].iter().find(|l| !l.is_empty())
    {
        return (*found).to_string();
    }

    url.split("session=")
        .nth(1)
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .take(5)
                .collect::<String>()
                .to_ascii_uppercase()
        })
        .unwrap_or_default()
}

/// 브라우저에서 받은 코드로 로그인을 끝낸다.
pub fn browser_complete_in<F>(
    provider: Provider,
    stage: &std::path::Path,
    code: &str,
    on_line: F,
) -> io::Result<Observation>
where
    F: Fn(exec::Stream, String) + Sync,
{
    only_firebase(provider)?;

    let code = code.trim();
    if code.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "코드를 입력하세요",
        ));
    }

    if !stage.is_dir() {
        return Err(io::Error::other("로그인을 먼저 시작하세요"));
    }

    let program = tools::find_in_path("firebase")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "firebase 를 찾을 수 없습니다"))?;

    // 첫 단계와 같은 설정 홈이어야 한다. 세션과 검증자가 거기 들어 있다.
    let buffer = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = buffer.clone();

    let outcome = exec::run_env(
        &program,
        &["login", code],
        &env_for(provider, stage),
        move |stream, line| {
            if let Ok(mut buf) = sink.lock() {
                buf.push_str(&line);
                buf.push('\n');
            }
            on_line(stream, line);
        },
    )?;

    if !outcome.ok() {
        // CLI 가 말한 이유를 그대로 전한다. 우리 말로 바꾸면 원인을 잃는다.
        let detail = buffer
            .lock()
            .ok()
            .and_then(|b| {
                b.lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_default();

        return Err(io::Error::other(format!(
            "코드로 로그인하지 못했습니다. 코드는 몇 분 안에 만료되므로 로그인을 다시 시작해 새 코드를 받으세요. ({detail})"
        )));
    }
    probe_home(provider, stage)
}

/// 출력에서 첫 번째 https 주소를 뽑는다.
fn first_url(text: &str) -> Option<String> {
    let start = text.find("https://")?;
    let rest = &text[start..];
    // 공백이나 줄바꿈에서 끊는다. CLI 가 주소 뒤에 안내를 붙이는 경우가 있다.
    let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extracts_the_auth_url_from_cli_output() {
        let out = "To sign in:\n 1. session ID: BDEC1\n 2. Visit:\n   https://auth.firebase.tools/login?code_challenge=abc&session=xyz\n 3. run firebase login <code>";
        assert_eq!(
            first_url(out).unwrap(),
            "https://auth.firebase.tools/login?code_challenge=abc&session=xyz"
        );
        assert!(first_url("주소가 없는 출력").is_none());
    }

    #[test]
    fn finds_the_session_id_to_match_in_the_browser() {
        let note = "To sign in to the Firebase CLI:\n\n1. Take note of your session ID:\n\n   DA2F7\n\n2. Visit the URL below";
        let url = "https://auth.firebase.tools/login?session=da2f7141-e311-4917";
        assert_eq!(session_id(note, url), "DA2F7");

        // 출력 형식이 바뀌어도 주소에서 같은 값을 뽑는다.
        assert_eq!(session_id("안내가 달라졌다", url), "DA2F7");
        assert_eq!(session_id("", "주소도 없다"), "");
    }

}
