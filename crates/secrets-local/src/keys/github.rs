//! GitHub 에 키를 등록하고 지우는 일.
//!
//! 전부 그 계정의 격리 홈으로 `gh` 를 돌린다. 지금 전역으로 활성화된 계정이
//! 무엇이든 상관없이, 사용자가 화면에서 고른 계정으로 나간다.

use secrets_core::key::{KeyError, KeyGateway, KeyRef, RemoteKey, RepoRef};
use secrets_core::port::{Channel, ProgressSink};

use crate::cli::{exec, tools};
use crate::vault::paths;

pub struct GhKeys;

fn remote(e: impl std::fmt::Display) -> KeyError {
    KeyError::Remote(e.to_string())
}

/// 그 계정의 격리 홈으로 `gh` 를 돌리고 stdout 을 모은다.
///
/// stdout 과 stderr 를 섞지 않는다. 응답을 파싱해야 하는데 경고 한 줄이 끼면
/// JSON 이 깨진다.
fn gh(
    account: &str,
    args: &[&str],
    stdin_data: Option<&[u8]>,
    progress: &dyn ProgressSink,
) -> Result<String, KeyError> {
    let program = tools::find_in_path("gh")
        .ok_or_else(|| KeyError::Remote("gh 를 찾을 수 없습니다".into()))?;

    let home = paths::cli_home_of(secrets_core::account::Provider::Github, account);
    let env = paths::env_for(secrets_core::account::Provider::Github, &home);

    progress.line(Channel::Out, &format!("$ {}", exec::display("gh", args)));

    let buffer = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = buffer.clone();

    let outcome = exec::run_full(&program, args, &env, stdin_data, move |stream, line| {
        match stream {
            exec::Stream::Stdout => {
                let mut held = sink.lock().unwrap();
                held.push_str(&line);
                held.push('\n');
            }
            // 오류는 사람이 보는 창으로 그대로 흘린다. 삼키면 왜 실패했는지 알 수 없다.
            exec::Stream::Stderr => progress.line(Channel::Err, &line),
        }
    })
    .map_err(remote)?;

    let text = buffer.lock().unwrap().clone();
    if !outcome.ok() {
        return Err(KeyError::Remote(format!(
            "gh 가 실패했습니다 ({})",
            outcome.code.unwrap_or(-1)
        )));
    }
    Ok(text)
}

/// `gh api` 응답에서 필요한 값만 꺼낸다.
///
/// JSON 라이브러리를 들이지 않고 `--jq` 로 gh 에게 시킨다. gh 가 이미 jq 를 품고
/// 있어서, 여기서 파서를 하나 더 두면 같은 일을 두 벌로 갖게 된다.
fn lines(text: &str) -> Vec<&str> {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).collect()
}

fn remote_keys(text: &str, repo: Option<&str>) -> Vec<RemoteKey> {
    // `--jq` 로 탭 구분 한 줄씩 받는다.
    lines(text)
        .into_iter()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let id = parts.next()?.to_string();
            let title = parts.next().unwrap_or_default().to_string();
            let key = parts.next().unwrap_or_default().to_string();
            let read_only = parts.next().unwrap_or("true") == "true";
            let created_at = parts.next().filter(|s| !s.is_empty()).map(str::to_string);
            Some(RemoteKey {
                id,
                title,
                fingerprint: fingerprint_of(&key),
                repo: repo.map(str::to_string),
                write: !read_only,
                created_at,
            })
        })
        .collect()
}

/// 공개 키 본문에서 지문을 계산한다.
///
/// GitHub 은 지문을 주지 않는다. 로컬 키와 맞춰 보려면 같은 방식으로 계산해야
/// 하므로, 여기서도 `ssh-keygen` 에게 시킨다.
fn fingerprint_of(public_key: &str) -> String {
    let Some(program) = tools::find_in_path("ssh-keygen") else {
        return String::new();
    };

    let buffer = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = buffer.clone();
    let outcome = exec::run_full(
        &program,
        &["-l", "-f", "-"],
        &[],
        Some(public_key.as_bytes()),
        move |stream, line| {
            if stream == exec::Stream::Stdout {
                sink.lock().unwrap().push_str(&line);
            }
        },
    );

    if !matches!(&outcome, Ok(o) if o.ok()) {
        return String::new();
    }
    let text = buffer.lock().unwrap().clone();
    text.split_whitespace()
        .find(|part| part.starts_with("SHA256:"))
        .unwrap_or_default()
        .to_string()
}

/// GitHub 에 올릴 제목. 끝에 올린 날을 붙인다.
///
/// 재발급하면 옛 키와 새 키가 GitHub 설정 화면에 잠시 나란히 있다. 날짜가 둘을 가른다.
/// 로컬 경로에는 붙이지 않는다 — 리포의 `core.sshCommand` 가 그 경로를 가리킨다.
fn title_of(at: &KeyRef, today: &str) -> String {
    let stamp: String = today.chars().filter(char::is_ascii_digit).collect();
    format!("secrets/{}-{stamp}", at.purpose)
}

impl KeyGateway for GhKeys {
    fn register(
        &self,
        account: &str,
        at: &KeyRef,
        public_key: &str,
        write: bool,
        progress: &dyn ProgressSink,
    ) -> Result<String, KeyError> {
        let path = format!("repos/{}/keys", at.repo.slug());
        let title = title_of(at, &crate::clock::today());
        let read_only = if write { "false" } else { "true" };

        let text = gh(
            account,
            &[
                "api",
                &path,
                "--method",
                "POST",
                "-f",
                &format!("title={title}"),
                "-f",
                &format!("key={public_key}"),
                "-F",
                &format!("read_only={read_only}"),
                "--jq",
                ".id",
            ],
            None,
            progress,
        )?;

        lines(&text)
            .first()
            .map(|id| id.to_string())
            .ok_or_else(|| KeyError::Remote("등록 결과에서 id 를 찾지 못했습니다".into()))
    }

    fn unregister(
        &self,
        account: &str,
        repo: &RepoRef,
        remote_id: &str,
        progress: &dyn ProgressSink,
    ) -> Result<(), KeyError> {
        let path = format!("repos/{}/keys/{remote_id}", repo.slug());
        gh(account, &["api", &path, "--method", "DELETE"], None, progress).map(|_| ())
    }

    fn deploy_keys(
        &self,
        account: &str,
        repo: &RepoRef,
        progress: &dyn ProgressSink,
    ) -> Result<Vec<RemoteKey>, KeyError> {
        let path = format!("repos/{}/keys", repo.slug());
        let text = gh(
            account,
            &[
                "api",
                &path,
                "--paginate",
                "--jq",
                r#".[] | [.id, .title, .key, .read_only, .created_at] | @tsv"#,
            ],
            None,
            progress,
        )?;
        Ok(remote_keys(&text, Some(&repo.slug())))
    }

    fn account_keys(
        &self,
        account: &str,
        progress: &dyn ProgressSink,
    ) -> Result<Vec<RemoteKey>, KeyError> {
        let text = gh(
            account,
            &[
                "api",
                "user/keys",
                "--paginate",
                "--jq",
                r#".[] | [.id, .title, .key, "true", ""] | @tsv"#,
            ],
            None,
            progress,
        )?;
        Ok(remote_keys(&text, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_ends_with_the_day_it_was_registered() {
        let at = KeyRef::new(RepoRef::parse("o/r").unwrap(), "develop").unwrap();
        assert_eq!(title_of(&at, "2026-09-24"), "secrets/develop-20260924");
    }
}
