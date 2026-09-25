//! SSH 접속 설정의 파일들 — 별칭 기록, 그룹별 conf, 그리고 `~/.ssh/config` 의 `Include` 한 줄.
//!
//! `~/.ssh/config` 에서 바꾸는 것은 `Include` 한 줄을 맨 위에 더하는 일뿐이다. 더하기 전에 원본을
//! 보관소로 복사하고, 파일이 심볼릭 링크면 링크가 가리키는 실제 파일에 쓴다.

use std::io::Write;
use std::path::{Path, PathBuf};

use secrets_core::ssh::{SshError, SshFiles, SshHost, SshStore, UserConfig};
use serde::{Deserialize, Serialize};

use crate::{clock, vault};

pub const HOSTS: &str = "hosts.toml";

pub struct FileSsh {
    /// 기록과 conf 파일이 놓이는 곳 (`~/.secrets/ssh`).
    pub dir: PathBuf,
    /// 사용자의 `~/.ssh/config`.
    pub user_config: PathBuf,
    /// `~/.ssh/config` 원본을 보관하는 곳.
    pub archive: PathBuf,
}

impl FileSsh {
    pub fn standard() -> FileSsh {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        FileSsh {
            dir: vault::root().join("ssh"),
            user_config: home.join(".ssh/config"),
            archive: vault::root().join("archive").join("ssh-config"),
        }
    }

    /// `~/.ssh/config` 에 넣을 줄. 홈 아래면 `~` 로 적는다.
    pub fn include_line(&self) -> String {
        let pattern = self.dir.join("*.conf").display().to_string();
        let shown = match std::env::var("HOME") {
            Ok(home) if !home.is_empty() && pattern.starts_with(&home) => {
                format!("~{}", &pattern[home.len()..])
            }
            _ => pattern,
        };
        format!("Include {shown}")
    }

    fn conf_of(&self, group: &str) -> PathBuf {
        self.dir.join(format!("{group}.conf"))
    }
}

fn storage(e: impl std::fmt::Display) -> SshError {
    SshError::Storage(e.to_string())
}

/// 임시 파일에 다 쓴 뒤 제자리로 옮긴다. 같은 디렉토리 안이라 rename 이 원자적이다.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), SshError> {
    let staging = path.with_extension("writing");
    {
        let mut file = std::fs::File::create(&staging).map_err(storage)?;
        file.write_all(bytes).map_err(storage)?;
        file.sync_all().map_err(storage)?;
    }
    vault::restrict(&staging).map_err(storage)?;
    if let Err(e) = std::fs::rename(&staging, path) {
        let _ = std::fs::remove_file(&staging);
        return Err(storage(e));
    }
    Ok(())
}

#[derive(Default, Serialize, Deserialize)]
struct Record {
    #[serde(default)]
    hosts: Vec<SshHost>,
}

impl SshStore for FileSsh {
    fn load(&self) -> Result<Vec<SshHost>, SshError> {
        let Ok(text) = std::fs::read_to_string(self.dir.join(HOSTS)) else {
            return Ok(Vec::new());
        };
        let record: Record = toml::from_str(&text).map_err(storage)?;
        Ok(record.hosts)
    }

    fn save(&self, hosts: &[SshHost]) -> Result<(), SshError> {
        vault::create_private(&self.dir).map_err(storage)?;
        let text = toml::to_string_pretty(&Record {
            hosts: hosts.to_vec(),
        })
        .map_err(storage)?;
        write_atomically(&self.dir.join(HOSTS), text.as_bytes())
    }
}

impl SshFiles for FileSsh {
    fn groups(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut groups: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .filter_map(|name| name.strip_suffix(".conf").map(str::to_string))
            .collect();
        groups.sort();
        groups
    }

    fn write_group(&self, group: &str, text: &str) -> Result<(), SshError> {
        vault::create_private(&self.dir).map_err(storage)?;
        write_atomically(&self.conf_of(group), text.as_bytes())
    }

    fn remove_group(&self, group: &str) -> Result<(), SshError> {
        // 기록에서 다시 만들 수 있는 파일이라 보관하지 않는다.
        match std::fs::remove_file(self.conf_of(group)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(storage(e)),
        }
    }

    fn user_config(&self) -> UserConfig {
        let Ok(text) = std::fs::read_to_string(&self.user_config) else {
            return UserConfig::default();
        };
        read_user_config(&text, &self.include_line())
    }

    fn add_include(&self) -> Result<Option<String>, SshError> {
        let line = self.include_line();
        // 심볼릭 링크면 링크가 가리키는 파일에 쓴다. 링크를 파일로 바꿔 버리지 않는다.
        let target =
            std::fs::canonicalize(&self.user_config).unwrap_or_else(|_| self.user_config.clone());
        let existing = std::fs::read_to_string(&target).ok();

        let kept = match &existing {
            Some(text) => {
                let place = self.archive.join(clock::stamp());
                vault::create_private(&place).map_err(storage)?;
                let copy = place.join("config");
                std::fs::write(&copy, text).map_err(storage)?;
                vault::restrict(&copy).map_err(storage)?;
                Some(copy.display().to_string())
            }
            None => {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(storage)?;
                    vault::restrict(parent).map_err(storage)?;
                }
                None
            }
        };

        let body = existing.unwrap_or_default();
        let text = format!(
            "# 인프라 콘솔 — 그룹별 SSH 설정을 합친다. 이 줄은 맨 위에 두어야 한다.\n{line}\n\n{body}"
        );
        write_atomically(&target, text.as_bytes())?;
        Ok(kept)
    }
}

/// `~/.ssh/config` 를 읽는다 — 우리 `Include` 가 있는지, 직접 적힌 Host 별칭은 무엇인지.
fn read_user_config(text: &str, include_line: &str) -> UserConfig {
    let wanted: Vec<String> = include_line
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let mut config = UserConfig::default();
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some(keyword) = words.first() else {
            continue;
        };
        if keyword.eq_ignore_ascii_case("include") && words.len() == 2 && words[1] == wanted[1] {
            config.includes_ours = true;
        }
        if keyword.eq_ignore_ascii_case("host") {
            config.aliases.extend(
                words[1..]
                    .iter()
                    .filter(|a| !a.contains(['*', '?', '!']))
                    .map(|a| a.to_string()),
            );
        }
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::tests_support::TempDir;

    fn files(dir: &TempDir) -> FileSsh {
        FileSsh {
            dir: dir.path().join("vault/ssh"),
            user_config: dir.path().join("home/.ssh/config"),
            archive: dir.path().join("vault/archive/ssh-config"),
        }
    }

    #[test]
    fn records_round_trip_and_conf_files_are_private() {
        let dir = TempDir::new("ssh-store");
        let ssh = files(&dir);
        let hosts = vec![SshHost {
            alias: "web".into(),
            group: "공구경".into(),
            instance: "i-1".into(),
            login: "admin".into(),
        }];
        ssh.save(&hosts).unwrap();
        ssh.write_group("공구경", "Host web\n").unwrap();

        assert_eq!(ssh.load().unwrap(), hosts);
        assert_eq!(ssh.groups(), vec!["공구경"]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(ssh.dir.join("공구경.conf"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        ssh.remove_group("공구경").unwrap();
        assert!(ssh.groups().is_empty());
    }

    #[test]
    fn include_goes_on_top_keeps_the_rest_and_the_original_is_archived() {
        let dir = TempDir::new("ssh-include");
        let ssh = files(&dir);
        dir.write(
            "home/.ssh/config",
            "Host github.com\n    IdentityFile ~/.ssh/github/main\n",
        );

        let kept = ssh.add_include().unwrap().expect("원본을 보관해야 한다");

        let text = std::fs::read_to_string(&ssh.user_config).unwrap();
        let first_setting = text
            .lines()
            .find(|l| !l.starts_with('#') && !l.trim().is_empty())
            .unwrap();
        assert!(first_setting.starts_with("Include "), "{text}");
        assert!(text.ends_with("Host github.com\n    IdentityFile ~/.ssh/github/main\n"));
        assert_eq!(
            std::fs::read_to_string(kept).unwrap(),
            "Host github.com\n    IdentityFile ~/.ssh/github/main\n"
        );
        assert!(ssh.user_config().includes_ours);
        assert_eq!(ssh.user_config().aliases, vec!["github.com"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_config_stays_a_symlink() {
        let dir = TempDir::new("ssh-link");
        let ssh = files(&dir);
        dir.write("dotfiles/ssh_config", "Host a\n");
        std::fs::create_dir_all(dir.path().join("home/.ssh")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("dotfiles/ssh_config"), &ssh.user_config)
            .unwrap();

        ssh.add_include().unwrap();

        assert!(
            std::fs::symlink_metadata(&ssh.user_config)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let real = std::fs::read_to_string(dir.path().join("dotfiles/ssh_config")).unwrap();
        assert!(real.contains("Include ") && real.ends_with("Host a\n"));
    }

    #[test]
    fn a_missing_config_is_created_with_just_the_include() {
        let dir = TempDir::new("ssh-new");
        let ssh = files(&dir);
        assert_eq!(ssh.add_include().unwrap(), None);
        assert!(ssh.user_config().includes_ours);
    }

    #[test]
    fn patterns_are_not_aliases() {
        let config = read_user_config("Host * !skip\nHost a b\nhost c\n", "Include ~/x/*.conf");
        assert_eq!(config.aliases, vec!["a", "b", "c"]);
        assert!(!config.includes_ours);
    }
}
