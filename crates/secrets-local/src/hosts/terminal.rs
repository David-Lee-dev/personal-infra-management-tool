//! 인스턴스 계정으로 들어가는 터미널 창을 연다. Ghostty 만 쓴다.
//!
//! 실행 중인 Ghostty 에 AppleScript 로 새 창을 연다(`open -na` 는 앱을 하나 더 띄운다).
//! 값은 argv 로 넘긴다 — 스크립트 문자열에 끼워 넣으면 따옴표 하나로 스크립트가 깨진다.

use std::path::Path;

use secrets_core::aws::instance::HostError;

use crate::cli::{exec, tools};

const SCRIPT: [&str; 9] = [
    "on run argv",
    "tell application \"Ghostty\"",
    "activate",
    "set cfg to new surface configuration",
    "set command of cfg to item 1 of argv",
    // ssh 가 곧바로 실패해도 창이 닫히지 않아야 이유를 읽을 수 있다.
    "set wait after command of cfg to true",
    "new window with configuration cfg",
    "end tell",
    "end run",
];

/// 창에서 돌 명령 한 줄.
///
/// Ghostty 는 이 줄을 `bash -c "exec -l …"` 로 돌린다. 셸이 읽으므로 값에는 셸이 달리
/// 읽을 글자가 하나도 없어야 한다 — 경로 · 계정 · 주소에 쓰는 글자만 받는다.
/// TERM 을 낮추는 까닭: Ghostty 는 `xterm-ghostty` 를 알리는데 그 terminfo 가 없는
/// 서버에서는 백스페이스 · 화면 지우기가 깨진다.
pub fn ssh_command(key: &Path, login: &str, address: &str) -> Result<String, HostError> {
    let key = key.display().to_string();
    for value in [key.as_str(), login, address] {
        let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | '@' | ':');
        if value.is_empty() || !value.chars().all(plain) {
            return Err(HostError::Storage(format!("터미널에 넘길 수 없는 값입니다: {value:?}")));
        }
    }
    Ok(format!(
        "/usr/bin/env TERM=xterm-256color /usr/bin/ssh -i {key} -o IdentitiesOnly=yes {login}@{address}"
    ))
}

/// Ghostty 새 창에서 그 계정으로 ssh 를 연다. 창을 연 뒤 곧바로 돌아온다.
pub fn open_ssh(key: &Path, login: &str, address: &str) -> Result<(), HostError> {
    if !key.is_file() {
        return Err(HostError::Storage(format!("{} 에 키가 없습니다", key.display())));
    }
    let command = ssh_command(key, login, address)?;
    let osascript = tools::find_in_path("osascript")
        .ok_or_else(|| HostError::Storage("osascript 를 찾을 수 없습니다".into()))?;

    let mut args: Vec<&str> = Vec::new();
    for line in SCRIPT {
        args.extend(["-e", line]);
    }
    args.push(&command);

    let said = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = said.clone();
    let outcome = exec::run(&osascript, &args, move |stream, line| {
        if stream == exec::Stream::Stderr {
            *sink.lock().unwrap() = line;
        }
    })
    .map_err(|e| HostError::Storage(e.to_string()))?;
    if !outcome.ok() {
        let said = said.lock().unwrap().clone();
        return Err(HostError::Storage(format!("Ghostty 창을 열지 못했습니다: {}", said.trim())));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_logs_in_with_that_key_only() {
        let line = ssh_command(Path::new("/k/key"), "david-admin", "3.37.156.50").unwrap();
        assert!(line.starts_with("/usr/bin/env "));
        assert!(line.ends_with("-i /k/key -o IdentitiesOnly=yes david-admin@3.37.156.50"));
        assert!(line.contains("TERM=xterm-256color"));
    }

    #[test]
    fn a_value_the_shell_would_read_differently_is_refused() {
        assert!(ssh_command(Path::new("/a b/key"), "u", "h").is_err());
        assert!(ssh_command(Path::new("/k"), "u x", "h").is_err());
        assert!(ssh_command(Path::new("/k"), "u", "h\n; rm").is_err());
        assert!(ssh_command(Path::new("/k"), "u", "").is_err());
        for nasty in ["$(id)", "`id`", "a;b", "a|b", "a&b", "a'b", "a\"b", "~/k", "a*"] {
            assert!(ssh_command(Path::new("/k"), nasty, "h").is_err(), "{nasty}");
        }
    }
}
