//! 프로젝트의 서버 — 시크릿 저장소의 서버 계정 목록과, 그 계정으로 배포 경로 읽기.
//!
//! 서버에는 쓰지 않는다. 읽는 스크립트는 경로 하나의 상태와 git 정보만 말한다.

use std::path::PathBuf;

use secrets_core::aws::instance::{AccountState, InstanceVault, Role, Seat};
use secrets_core::port::ProgressSink;
use secrets_core::project::{
    Checkout, CheckoutFacts, ProjectError, ServerProbe, ServerSeat, ServerSeats,
};

use crate::aws_vault;
use crate::hosts::{FileAccounts, script, ssh};

/// 서버 계정이 놓일 수 있는 머신 종류.
const MACHINES: &[&str] = &["ec2", "lightsail"];

pub struct VaultSeats;

fn dirs_in(path: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

impl ServerSeats for VaultSeats {
    fn seats(&self) -> Vec<ServerSeat> {
        let mut seats = Vec::new();
        for aws_account in dirs_in(&aws_vault::root()) {
            for machine in MACHINES {
                let vault = FileAccounts {
                    aws_account: aws_account.clone(),
                    machine: machine.to_string(),
                };
                for record in vault.list().into_iter().filter_map(Result::ok) {
                    let key_path = Seat::new(
                        &record.region,
                        &record.keypair,
                        &record.instance,
                        &record.account,
                    )
                    .map(|seat| vault.private_path(&seat))
                    .unwrap_or_default();
                    seats.push(ServerSeat {
                        key_path,
                        aws_account: aws_account.clone(),
                        machine: machine.to_string(),
                        region: record.region,
                        keypair: record.keypair,
                        instance: record.instance,
                        instance_name: record.instance_name,
                        address: record.address,
                        login: record.account,
                        admin: record.role == Role::Admin,
                        verified: record.state == AccountState::Verified,
                    });
                }
            }
        }
        seats
    }
}

pub struct SshProbe;

pub(super) fn private_key(seat: &ServerSeat) -> Result<PathBuf, ProjectError> {
    let place =
        Seat::new(&seat.region, &seat.keypair, &seat.instance, &seat.login).ok_or_else(|| {
            ProjectError::Invalid(format!(
                "{}은(는) 서버 계정 이름으로 쓸 수 없습니다.",
                seat.login
            ))
        })?;
    let vault = FileAccounts {
        aws_account: seat.aws_account.clone(),
        machine: seat.machine.clone(),
    };
    Ok(PathBuf::from(vault.private_path(&place)))
}

/// 배포 경로 하나를 읽는 스크립트. 바꾸는 명령은 없다.
///
/// 다른 계정 소유의 저장소도 origin 을 읽을 수 있게 `safe.directory` 를 이 명령에서만 푼다.
fn checkout_script(path: &str) -> String {
    format!(
        r#"set -u
p={path}
g() {{ git -c safe.directory='*' "$@" 2>/dev/null; }}
if [ ! -e "$p" ]; then echo "state=missing"; exit 0; fi
if [ ! -d "$p" ]; then echo "state=plain"; exit 0; fi
if [ ! -r "$p" ] || [ ! -x "$p" ]; then echo "state=unreadable"; exit 0; fi
if [ -e "$p/.git" ]; then
  echo "state=repository"
  echo "origin=$(g -C "$p" remote get-url origin)"
  echo "branch=$(g -C "$p" symbolic-ref --quiet --short HEAD)"
  echo "commit=$(g -C "$p" rev-parse --short HEAD)"
  echo "owner=$(stat -c %U "$p" 2>/dev/null || stat -f %Su "$p")"
  echo "group=$(stat -c %G "$p" 2>/dev/null || stat -f %Sg "$p")"
  echo "sshcommand=$(g -C "$p" config --get core.sshCommand)"
  exit 0
fi
if [ -z "$(ls -A "$p")" ]; then echo "state=empty"; exit 0; fi
echo "state=plain"
"#,
        path = script::quote(path),
    )
}

/// `state=…` · `origin=…` 줄을 읽는다. 빈 값은 없는 것으로 본다.
fn parse_checkout(text: &str) -> Result<Checkout, ProjectError> {
    let value = |key: &str| {
        text.lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix(&format!("{key}="))
                    .map(str::to_string)
            })
            .filter(|v| !v.is_empty())
    };
    match value("state").as_deref() {
        Some("missing") => Ok(Checkout::Missing),
        Some("empty") => Ok(Checkout::Empty),
        Some("plain") => Ok(Checkout::Plain),
        Some("unreadable") => Err(ProjectError::Invalid(
            "이 계정으로는 배포 경로를 읽을 수 없습니다(권한 없음). 배포 계정이 읽고 쓸 수 있는 경로를 입력하세요.".into(),
        )),
        Some("repository") => Ok(Checkout::Repository {
            origin: value("origin"),
            branch: value("branch"),
            commit: value("commit"),
            facts: CheckoutFacts {
                owner: value("owner"),
                group: value("group"),
                ssh_command: value("sshcommand"),
            },
        }),
        _ => Err(ProjectError::Storage("서버의 응답을 읽지 못했습니다.".into())),
    }
}

impl ServerProbe for SshProbe {
    fn checkout(
        &self,
        seat: &ServerSeat,
        path: &str,
        progress: &dyn ProgressSink,
    ) -> Result<Checkout, ProjectError> {
        let key = private_key(seat)?;
        let text = ssh::run(
            &key.display().to_string(),
            &seat.login,
            &seat.address,
            &checkout_script(path),
            progress,
        )
        .map_err(|e| ProjectError::Storage(e.to_string()))?;
        parse_checkout(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod parse_checkout {
        use super::*;

        #[test]
        fn reads_each_state() {
            assert_eq!(
                parse_checkout("state=missing\n").unwrap(),
                Checkout::Missing
            );
            assert_eq!(parse_checkout("state=plain\n").unwrap(), Checkout::Plain);
            assert_eq!(parse_checkout("state=empty\n").unwrap(), Checkout::Empty);
            assert!(parse_checkout("state=unreadable\n").is_err());
            assert_eq!(
                parse_checkout(
                    "state=repository\norigin=git@github.com:O/r.git\nbranch=main\ncommit=38d8077\nowner=deploy\ngroup=workspace\nsshcommand=ssh -i /home/deploy/.ssh/github/r -o IdentitiesOnly=yes\n"
                )
                .unwrap(),
                Checkout::Repository {
                    origin: Some("git@github.com:O/r.git".into()),
                    branch: Some("main".into()),
                    commit: Some("38d8077".into()),
                    facts: CheckoutFacts {
                        owner: Some("deploy".into()),
                        group: Some("workspace".into()),
                        ssh_command: Some("ssh -i /home/deploy/.ssh/github/r -o IdentitiesOnly=yes".into()),
                    },
                }
            );
        }

        #[test]
        fn empty_values_are_absent() {
            assert_eq!(
                parse_checkout("state=repository\norigin=\nbranch=\ncommit=\n").unwrap(),
                Checkout::Repository {
                    origin: None,
                    branch: None,
                    commit: None,
                    facts: CheckoutFacts::default(),
                }
            );
        }

        #[test]
        fn anything_else_is_an_error() {
            assert!(parse_checkout("Permission denied").is_err());
        }
    }

    mod checkout_script {
        use super::*;

        /// 스크립트를 이 머신의 셸로 돌려 본다. ssh 만 빼고 같은 것이다.
        fn run_here(path: &str) -> Checkout {
            let out = std::process::Command::new("bash")
                .arg("-c")
                .arg(checkout_script(path))
                .output()
                .unwrap();
            parse_checkout(&String::from_utf8_lossy(&out.stdout)).unwrap()
        }

        #[test]
        fn tells_missing_plain_and_repository_apart_on_a_real_shell() {
            let dir = crate::project::workspace::tests_support::TempDir::new("probe");
            dir.write("plain/a.txt", "x");
            dir.write("file.txt", "x");
            std::fs::create_dir(dir.path().join("empty")).unwrap();
            let none = Checkout::Plain;
            assert_eq!(
                run_here(&dir.path().join("nope").display().to_string()),
                Checkout::Missing
            );
            assert_eq!(
                run_here(&dir.path().join("empty").display().to_string()),
                Checkout::Empty
            );
            assert_eq!(
                run_here(&dir.path().join("plain").display().to_string()),
                none
            );
            assert_eq!(
                run_here(&dir.path().join("file.txt").display().to_string()),
                none
            );

            let repo = dir.path().join("repo");
            std::fs::create_dir(&repo).unwrap();
            let git = |args: &[&str]| {
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(&repo)
                    .args(args)
                    .output()
                    .unwrap()
            };
            git(&["init", "-q", "--initial-branch=main"]);
            git(&["remote", "add", "origin", "git@github.com:O/r.git"]);
            git(&[
                "config",
                "core.sshCommand",
                "ssh -i /k -o IdentitiesOnly=yes",
            ]);
            let Checkout::Repository {
                origin,
                branch,
                commit,
                facts,
            } = run_here(&repo.display().to_string())
            else {
                panic!("레포로 읽혀야 한다");
            };
            assert_eq!(origin.as_deref(), Some("git@github.com:O/r.git"));
            assert_eq!(branch.as_deref(), Some("main"));
            assert_eq!(commit, None);
            assert!(facts.owner.is_some() && facts.group.is_some());
            assert_eq!(
                facts.ssh_command.as_deref(),
                Some("ssh -i /k -o IdentitiesOnly=yes")
            );
        }

        #[test]
        fn a_path_with_quotes_and_spaces_is_passed_as_one_word() {
            let dir = crate::project::workspace::tests_support::TempDir::new("quote");
            dir.write("it's here/a.txt", "x");
            assert_eq!(
                run_here(&dir.path().join("it's here").display().to_string()),
                Checkout::Plain
            );
        }
    }
}
