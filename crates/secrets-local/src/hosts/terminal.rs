//! 서버 계정으로 들어가는 터미널 창을 연다. Ghostty 만 쓴다.
//!
//! 실행 중인 Ghostty 에 AppleScript 로 새 창을 연다(`open -na` 는 앱을 하나 더 띄운다).
//! 값은 argv 로 넘긴다 — 스크립트 문자열에 끼워 넣으면 따옴표 하나로 스크립트가 깨진다.

use secrets_core::server::{Access, ServerError};

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

/// 창에서 돌 명령 한 줄. 화면의 "ssh 명령 복사"도 이 줄을 쓴다.
///
/// Ghostty 는 이 줄을 `bash -c "exec -l …"` 로 돌린다. 셸이 읽으므로 값에는 셸이 달리
/// 읽을 글자가 하나도 없어야 한다 — 경로 · 계정 · 주소에 쓰는 글자만 받는다.
/// TERM 을 낮추는 까닭: Ghostty 는 `xterm-ghostty` 를 알리는데 그 terminfo 가 없는
/// 서버에서는 백스페이스 · 화면 지우기가 깨진다.
/// 키가 없으면 `-i` 를 빼고 ssh 기본 키(ssh-agent · `~/.ssh/id_*`)에 맡긴다.
pub fn ssh_command(access: &Access) -> Result<String, ServerError> {
    let plain =
        |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | '@' | ':');
    let values = [
        access.key.as_deref(),
        Some(access.login.as_str()),
        Some(access.address.as_str()),
    ];
    for value in values.into_iter().flatten() {
        if value.is_empty() || value.starts_with('-') || !value.chars().all(plain) {
            return Err(ServerError::Invalid(format!(
                "터미널에 전달할 수 없는 값입니다: {value:?}"
            )));
        }
    }
    let mut line = String::from("/usr/bin/env TERM=xterm-256color /usr/bin/ssh");
    if access.port != 22 {
        line.push_str(&format!(" -p {}", access.port));
    }
    if let Some(key) = &access.key {
        line.push_str(&format!(" -i {key} -o IdentitiesOnly=yes"));
    }
    line.push_str(&format!(" {}@{}", access.login, access.address));
    Ok(line)
}

/// Ghostty 새 창에서 그 계정으로 ssh 를 연다. 창을 연 뒤 곧바로 돌아온다.
pub fn open_ssh(access: &Access) -> Result<(), ServerError> {
    if let Some(key) = &access.key
        && !std::path::Path::new(key).is_file()
    {
        return Err(ServerError::Storage(format!("{key} 에 키가 없습니다")));
    }
    let command = ssh_command(access)?;
    let osascript = tools::find_in_path("osascript")
        .ok_or_else(|| ServerError::Storage("osascript 를 찾을 수 없습니다".into()))?;

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
    .map_err(|e| ServerError::Storage(e.to_string()))?;
    if !outcome.ok() {
        let said = said.lock().unwrap().clone();
        return Err(ServerError::Storage(format!(
            "Ghostty 창을 열지 못했습니다: {}",
            said.trim()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access(key: Option<&str>, login: &str, address: &str, port: u16) -> Access {
        Access {
            key: key.map(str::to_string),
            login: login.into(),
            address: address.into(),
            port,
        }
    }

    #[test]
    fn the_command_logs_in_with_that_key_only() {
        let line = ssh_command(&access(Some("/k/key"), "david-admin", "3.37.156.50", 22)).unwrap();
        assert_eq!(
            line,
            "/usr/bin/env TERM=xterm-256color /usr/bin/ssh -i /k/key -o IdentitiesOnly=yes david-admin@3.37.156.50"
        );
    }

    #[test]
    fn without_a_key_ssh_picks_its_own_and_another_port_is_named() {
        let line = ssh_command(&access(None, "infra", "nemo.tail25dc19.ts.net", 2222)).unwrap();
        assert_eq!(
            line,
            "/usr/bin/env TERM=xterm-256color /usr/bin/ssh -p 2222 infra@nemo.tail25dc19.ts.net"
        );
    }

    #[test]
    fn a_value_the_shell_would_read_differently_is_refused() {
        assert!(ssh_command(&access(Some("/a b/key"), "u", "h", 22)).is_err());
        assert!(ssh_command(&access(Some("/k"), "u x", "h", 22)).is_err());
        assert!(ssh_command(&access(Some("/k"), "u", "h\n; rm", 22)).is_err());
        assert!(ssh_command(&access(Some("/k"), "u", "", 22)).is_err());
        assert!(ssh_command(&access(Some("/k"), "u", "-oProxyCommand=x", 22)).is_err());
        for nasty in [
            "$(id)", "`id`", "a;b", "a|b", "a&b", "a'b", "a\"b", "~/k", "a*",
        ] {
            assert!(
                ssh_command(&access(Some("/k"), nasty, "h", 22)).is_err(),
                "{nasty}"
            );
        }
    }
}
