//! 커밋이 어느 계정으로 나가는가.
//!
//! `gh` 는 커밋 신원을 건드리지 않는다. 계정만 바꾸고 이걸 놔두면 커밋이 이전
//! 계정 이메일로 나가고, GitHub 에서 다른 사람 커밋으로 잡힌다.

use std::io;

use crate::cli::{exec, tools};

pub fn set_git_email(email: &str) -> io::Result<()> {
    let program = tools::find_in_path("git")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "git 을 찾을 수 없습니다"))?;

    let outcome = exec::run(
        &program,
        &["config", "--global", "user.email", email],
        |_, _| {},
    )?;
    if outcome.ok() {
        Ok(())
    } else {
        Err(io::Error::other("커밋 이메일을 바꾸지 못했습니다"))
    }
}

/// 지금 전역 git 커밋 이메일.
pub fn git_email() -> Option<String> {
    let program = tools::find_in_path("git")?;
    let buffer = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = buffer.clone();

    let outcome = exec::run(
        &program,
        &["config", "--global", "user.email"],
        move |_, line| {
            if let Ok(mut buf) = sink.lock() {
                buf.push_str(&line);
            }
        },
    )
    .ok()?;

    outcome
        .ok()
        .then(|| buffer.lock().ok().map(|b| b.trim().to_string()))
        .flatten()
        .filter(|s| !s.is_empty())
}

