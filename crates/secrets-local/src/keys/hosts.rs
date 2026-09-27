//! 이 머신이 아는 SSH 호스트.
//!
//! 키를 어느 기계로 보낼지 고르는 데 쓴다. 별칭을 쓰면 `~/.ssh/config` 의 User 와
//! 포트가 같이 걸려서 명령이 짧아지고, 손으로 IP 를 옮겨 적을 일이 없다.
//!
//! **읽기만 한다.** 이 파일은 이 도구 밖의 것이고, 사용자와 다른 도구가 함께 쓴다.

use std::path::PathBuf;

/// `~/.ssh/config` 에 적힌 호스트 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    /// `ssh <이것>` 으로 붙는 이름.
    pub alias: String,
    /// 실제 주소. 적혀 있지 않으면 별칭이 곧 주소다.
    pub address: Option<String>,
    pub user: Option<String>,
    /// 이 호스트가 내미는 개인 키. 파일을 옮기면 이 접속이 끊긴다.
    pub identity: Option<String>,
    pub port: Option<u16>,
    /// 위의 것 말고 이 호스트 아래 적힌 설정 줄(`LocalForward …` 등). 접속 자체에는 쓰지 않는
    /// 것으로 보는 `IdentitiesOnly` · `StrictHostKeyChecking` · `SetEnv` 는 뺀다.
    pub extras: Vec<String>,
}

fn config_path() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".ssh/config")
}

/// 적힌 순서대로 돌려준다. 사용자가 적어 둔 순서가 곧 그 사람의 분류다.
pub fn known() -> Vec<Host> {
    let Ok(text) = std::fs::read_to_string(config_path()) else {
        return Vec::new();
    };
    parse(&text)
}

fn parse(text: &str) -> Vec<Host> {
    let mut found: Vec<Host> = Vec::new();
    // 방금 만난 `Host` 줄이 연 항목들. 한 줄에 여러 별칭이 올 수 있다.
    let mut open: Vec<usize> = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((word, rest)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let rest = rest.trim();

        match word.to_ascii_lowercase().as_str() {
            "host" => {
                open.clear();
                for alias in rest.split_whitespace() {
                    // 패턴은 붙을 수 있는 이름이 아니다.
                    if alias.contains(['*', '?', '!']) {
                        continue;
                    }
                    open.push(found.len());
                    found.push(Host {
                        alias: alias.to_string(),
                        address: None,
                        user: None,
                        identity: None,
                        port: None,
                        extras: Vec::new(),
                    });
                }
            }
            "hostname" => {
                for at in &open {
                    found[*at].address = Some(rest.to_string());
                }
            }
            "user" => {
                for at in &open {
                    found[*at].user = Some(rest.to_string());
                }
            }
            "identityfile" => {
                for at in &open {
                    found[*at].identity = Some(rest.to_string());
                }
            }
            "port" => {
                for at in &open {
                    found[*at].port = rest.parse().ok();
                }
            }
            "identitiesonly" | "stricthostkeychecking" | "setenv" => {}
            _ => {
                for at in &open {
                    found[*at].extras.push(format!("{word} {rest}"));
                }
            }
        }
    }
    found
}

/// 이 개인 키를 가리키는 호스트들.
///
/// 파일을 옮기기 전에 **무엇이 끊기는지** 알아야 한다. `~/.ssh/config` 는 이 도구
/// 밖의 파일이라 우리가 고치지 않고, 대신 무엇이 걸려 있는지만 말해 준다.
pub fn referring_to(path: &std::path::Path) -> Vec<String> {
    let target = std::fs::canonicalize(path).ok();

    known()
        .into_iter()
        .filter(|host| match &host.identity {
            Some(identity) => {
                let at = expand(identity);
                match (&target, std::fs::canonicalize(&at).ok()) {
                    (Some(a), Some(b)) => *a == b,
                    _ => at == path,
                }
            }
            None => false,
        })
        .map(|host| host.alias)
        .collect()
}

fn expand(text: &str) -> PathBuf {
    match text.strip_prefix("~/") {
        Some(rest) => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(rest),
        None => PathBuf::from(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_carries_the_address_and_user_written_under_it() {
        let found = parse(
            "Host tukapp-prod\n    HostName 54.116.119.214\n    User ubuntu\n    IdentityFile ~/.ssh/tuk/personal\n",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].alias, "tukapp-prod");
        assert_eq!(found[0].address.as_deref(), Some("54.116.119.214"));
        assert_eq!(found[0].user.as_deref(), Some("ubuntu"));
        assert_eq!(found[0].identity.as_deref(), Some("~/.ssh/tuk/personal"));
    }

    #[test]
    fn one_line_can_open_several_hosts_and_they_all_get_the_settings() {
        let found = parse("Host nemo nemo-deploy\n  HostName nemo.ts.net\n");
        assert_eq!(found.len(), 2);
        assert!(
            found
                .iter()
                .all(|h| h.address.as_deref() == Some("nemo.ts.net"))
        );
    }

    #[test]
    fn patterns_are_not_hosts_you_can_connect_to() {
        // `Host *` 는 모든 호스트에 걸리는 기본값이지 붙을 수 있는 이름이 아니다.
        let found = parse("Host *\n  ServerAliveInterval 60\nHost real\n  HostName 10.0.0.1\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].alias, "real");
    }

    #[test]
    fn settings_under_a_pattern_do_not_leak_into_the_next_host() {
        let found = parse("Host *\n  User root\nHost real\n  HostName 10.0.0.1\n");
        assert_eq!(
            found[0].user, None,
            "패턴 아래의 값이 다음 항목에 붙으면 안 된다"
        );
    }

    #[test]
    fn a_port_and_the_settings_that_are_not_moved_are_kept() {
        let found = parse(
            "Host vpn\n  HostName 1.2.3.4\n  Port 2222\n  IdentitiesOnly yes\n  LocalForward 51821 127.0.0.1:51821\n",
        );
        assert_eq!(found[0].port, Some(2222));
        assert_eq!(found[0].extras, vec!["LocalForward 51821 127.0.0.1:51821"]);
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let found = parse("# >>> managed >>>\n\nHost a\n  HostName 1.2.3.4\n# <<< managed <<<\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].address.as_deref(), Some("1.2.3.4"));
    }
}
