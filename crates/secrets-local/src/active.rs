//! 전역 계정 전환.
//!
//! CLI 들은 설정을 정해진 경로에서 읽는다. 그 경로를 계정 홈으로 **심볼릭 링크**
//! 하고, 전환은 링크를 갈아끼우는 것으로 한다. 설정을 복사하지 않으므로 원본이
//! 하나뿐이고, 이미 열려 있는 터미널도 다음 명령부터 바뀐다.
//!
//! 환경변수(`GH_CONFIG_DIR` 등)가 이 링크보다 우선하므로, 특정 터미널만 다른
//! 계정을 쓰는 것도 충돌 없이 된다. 전역은 링크, 터미널별은 환경변수다.

use std::io;
use std::path::{Path, PathBuf};

use secrets_core::account::{Account, Provider};
use crate::{exec, home, tools};
use crate::clock;
use crate::paths;

/// 전역 전환으로 갈아끼울 경로 한 쌍.
pub struct Link {
    /// CLI 가 읽는 자리. 여기에 링크를 건다.
    pub global: PathBuf,
    /// 계정 안에서 그 자리에 대응하는 실물.
    pub source: PathBuf,
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"))
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// 이 provider 를 전역으로 전환할 수 있는가. 할 수 있으면 갈아끼울 경로를 준다.
pub fn link_for(account: &Account) -> Option<Link> {
    let cli = paths::cli_home(account);
    Some(match account.provider {
        Provider::Github => Link {
            global: config_home().join("gh"),
            source: cli,
        },
        Provider::Aws => Link {
            global: home_dir().join(".aws"),
            source: cli,
        },
        Provider::Gcloud => Link {
            global: config_home().join("gcloud"),
            source: cli,
        },
        // firebase 는 XDG_CONFIG_HOME 아래 configstore 를 쓴다. 격리 홈에서도
        // 그 하위에 쓰이므로 한 단계 더 들어간다.
        Provider::Firebase => Link {
            global: config_home().join("configstore"),
            source: cli.join("configstore"),
        },
    })
}

/// 전역 전환이 다른 도구에 영향을 줄 수 있으면 그 이유. 없으면 None.
///
/// 막지는 않는다. 사용자가 알고 고르게만 한다.
pub fn caution(provider: Provider) -> Option<&'static str> {
    match provider {
        // configstore 는 firebase 전용이 아니다. 같은 디렉토리를 쓰는 다른 도구가
        // 있으면 그 설정까지 함께 옮겨 간다.
        Provider::Firebase => Some(
            "firebase 는 다른 도구와 같은 configstore 디렉토리를 씁니다. 전역으로 바꾸면 그 도구들의 설정도 함께 옮겨 갑니다.",
        ),
        _ => None,
    }
}

/// 지금 전역으로 활성화된 계정의 슬러그.
///
/// 별도 상태 파일을 두지 않는다. 링크가 어디를 가리키는지가 곧 답이고,
/// 밖에서 누가 바꿔도 그대로 읽힌다.
pub fn active_slug(provider: Provider) -> Option<String> {
    let probe = Account::new(provider, "-");
    let link = link_for(&probe)?;
    let target = std::fs::read_link(&link.global).ok()?;

    // <root>/accounts/<provider>/<slug>/cli[/...] 형태여야 우리 것이다.
    let accounts = home::root().join(home::ACCOUNTS).join(provider.id());
    let rest = target.strip_prefix(&accounts).ok()?;
    rest.components()
        .next()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
}

pub fn is_active(account: &Account) -> bool {
    active_slug(account.provider).as_deref() == Some(account.slug.as_str())
}

/// 전환 결과. 무슨 일이 있었는지 호출자가 사용자에게 알릴 수 있게 남긴다.
#[derive(Debug, Default)]
pub struct Switched {
    /// 기존 실물을 치웠다면 그 보관 위치.
    pub archived: Option<PathBuf>,
    /// 링크를 건 자리.
    pub linked: PathBuf,
    /// 커밋 이메일을 바꿨다면 그 값.
    pub git_email: Option<String>,
}

/// 전역 git 커밋 이메일을 바꾼다.
///
/// 계정을 바꿔도 이걸 놔두면 커밋이 이전 계정 이메일로 나간다.
/// 전역 설정을 건드리는 일이라 계정에 이메일이 적혀 있을 때만 한다.
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

/// 이 계정을 전역으로 활성화한다.
///
/// 자리에 실물이 있으면 **지우지 않고 보관소로 옮긴다.** 링크로 덮어쓰면
/// 아직 레지스트리에 없는 로그인이 소리 없이 사라진다.
pub fn activate(account: &Account) -> io::Result<Switched> {
    let link = link_for(account).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            format!("{} 는 전역 전환을 지원하지 않습니다", account.provider.id()),
        )
    })?;

    if !link.source.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "이 계정에 CLI 설정이 없습니다. 먼저 연결하세요",
        ));
    }

    let mut result = Switched {
        archived: None,
        linked: link.global.clone(),
        git_email: None,
    };

    match std::fs::symlink_metadata(&link.global) {
        Ok(meta) if meta.file_type().is_symlink() => {
            // 우리가 건 링크든 남의 링크든, 링크는 그냥 갈아끼운다.
            std::fs::remove_file(&link.global)?;
        }
        Ok(_) => {
            result.archived = Some(archive(account.provider, &link.global)?);
        }
        Err(_) => {}
    }

    if let Some(parent) = link.global.parent() {
        std::fs::create_dir_all(parent)?;
    }
    symlink(&link.source, &link.global)?;

    // 링크가 걸린 뒤에 신원을 맞춘다. 이메일이 적혀 있지 않으면 건드리지 않는다.
    if let Some(email) = account
        .git_email
        .as_deref()
        .filter(|e| !e.trim().is_empty())
    {
        set_git_email(email)?;
        result.git_email = Some(email.to_string());
    }

    Ok(result)
}

/// 전역 링크를 걷어낸다. 계정 쪽 실물은 건드리지 않는다.
pub fn deactivate(provider: Provider) -> io::Result<()> {
    let probe = Account::new(provider, "-");
    let Some(link) = link_for(&probe) else {
        return Ok(());
    };
    match std::fs::symlink_metadata(&link.global) {
        Ok(meta) if meta.file_type().is_symlink() => std::fs::remove_file(&link.global),
        _ => Ok(()),
    }
}

/// 자리에 있던 실물을 보관소로 옮긴다.
fn archive(provider: Provider, path: &Path) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| provider.id().to_string());

    let dir = unique(
        home::root()
            .join("archive")
            .join(provider.id())
            .join(clock::today()),
    );
    home::create_private(&dir)?;

    let target = dir.join(name);
    std::fs::rename(path, &target)?;
    Ok(target)
}

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

#[cfg(unix)]
fn symlink(source: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(source, link)
}

#[cfg(not(unix))]
fn symlink(_source: &Path, _link: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "이 운영체제는 지원하지 않습니다",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;
    use crate::home::tests_support::with_temp_root;

    /// 전역 경로도 임시로 옮긴다. 실제 ~/.config 를 건드리면 안 된다.
    fn with_fake_home<T>(body: impl FnOnce() -> T) -> T {
        with_temp_root(|root| {
            let fake = root.join("fake-home");
            std::fs::create_dir_all(fake.join(".config")).unwrap();
            // SAFETY: with_temp_root 의 잠금 안이라 다른 테스트와 겹치지 않는다.
            unsafe {
                std::env::set_var("HOME", &fake);
                std::env::remove_var("XDG_CONFIG_HOME");
            }
            let out = body();
            unsafe { std::env::remove_var("HOME") };
            out
        })
    }

    fn connected(provider: Provider, slug: &str) -> Account {
        let account = Account::new(provider, slug);
        store::save(&account).unwrap();
        // 연결된 척. activate 는 실물이 있어야 한다.
        std::fs::write(paths::cli_home(&account).join("hosts.yml"), "x").unwrap();
        account
    }

    #[test]
    fn activate_points_the_global_path_at_the_account() {
        with_fake_home(|| {
            let account = connected(Provider::Github, "personal");
            let result = activate(&account).unwrap();

            assert!(result.archived.is_none(), "치울 실물이 없었다");
            assert!(result.linked.is_symlink());
            assert_eq!(
                std::fs::read_link(&result.linked).unwrap(),
                paths::cli_home(&account)
            );
            assert!(is_active(&account));
            assert_eq!(active_slug(Provider::Github).as_deref(), Some("personal"));
        });
    }

    #[test]
    fn switching_between_accounts_just_repoints_the_link() {
        with_fake_home(|| {
            let a = connected(Provider::Github, "personal");
            let b = connected(Provider::Github, "work");

            activate(&a).unwrap();
            let result = activate(&b).unwrap();

            // 링크를 갈아끼울 뿐이므로 보관할 것이 생기지 않는다.
            assert!(result.archived.is_none(), "링크는 보관하지 않는다");
            assert!(is_active(&b));
            assert!(!is_active(&a));
        });
    }

    #[test]
    fn existing_config_is_archived_not_destroyed() {
        with_fake_home(|| {
            let account = connected(Provider::Github, "personal");

            // 레지스트리 밖에서 쓰던 실제 로그인이 자리에 있다고 하자.
            let global = link_for(&account).unwrap().global;
            std::fs::create_dir_all(&global).unwrap();
            std::fs::write(global.join("hosts.yml"), "이전 로그인").unwrap();

            let result = activate(&account).unwrap();

            let archived = result.archived.expect("실물은 보관되어야 한다");
            assert_eq!(
                std::fs::read_to_string(archived.join("hosts.yml")).unwrap(),
                "이전 로그인",
                "내용이 그대로 남아야 한다"
            );
            assert!(is_active(&account));
        });
    }

    #[test]
    fn deactivate_removes_the_link_but_keeps_the_account() {
        with_fake_home(|| {
            let account = connected(Provider::Github, "personal");
            activate(&account).unwrap();

            deactivate(Provider::Github).unwrap();

            assert!(active_slug(Provider::Github).is_none());
            assert!(
                paths::cli_home(&account).join("hosts.yml").is_file(),
                "계정은 남는다"
            );
        });
    }

    #[test]
    fn refuses_to_activate_an_unconnected_account() {
        with_fake_home(|| {
            let account = Account::new(Provider::Github, "empty");
            store::save(&account).unwrap();
            // cli 홈은 만들어지지만 비어 있다 — 링크를 걸면 로그인 없는 상태가 전역이 된다.
            std::fs::remove_dir_all(paths::cli_home(&account)).unwrap();

            assert!(activate(&account).is_err());
        });
    }

    #[test]
    fn activate_leaves_commit_identity_alone_when_unset() {
        with_fake_home(|| {
            let account = connected(Provider::Github, "personal");
            // 이메일을 적지 않은 계정은 전역 git 설정을 건드리지 않는다.
            assert!(account.git_email.is_none());
            assert!(activate(&account).unwrap().git_email.is_none());
        });
    }

    #[test]
    fn firebase_links_one_level_deeper_and_carries_a_caution() {
        with_fake_home(|| {
            let account = Account::new(Provider::Firebase, "tuk");
            store::save(&account).unwrap();
            let link = link_for(&account).unwrap();

            assert!(link.source.ends_with("cli/configstore"));
            assert!(link.global.ends_with(".config/configstore"));
            assert!(caution(Provider::Firebase).is_some());
            assert!(caution(Provider::Github).is_none());
        });
    }
}
