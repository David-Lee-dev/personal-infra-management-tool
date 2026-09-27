//! 환경 변수 동기화 — 로컬 뿌리의 파일 하나를 서버 배포 경로 뿌리의 파일 하나로 올린다.
//!
//! 로컬이 정본이다. 환경마다 로컬 파일 하나와 서버에 둘 이름을 고른다(예: prod 는 `.env.prod` 를
//! 서버의 `.env`). 서버 쪽은 언제나 배포 경로의 뿌리다 — 런타임이 디버그 때와 같은 자리에서 읽게.
//! 이름은 런타임이 읽는 이름이라 사용자가 정한다(`.env` · `.env.local` …).
//!
//! 비교는 값을 옮기지 않는다. 양쪽에서 같은 방식으로 파일 전체와 변수마다의 해시를 만들고,
//! 해시는 비교 한 번에만 쓰는 임의 값(salt)을 섞어 기록에 남아도 값을 되짚을 수 없게 한다.
//! 일치는 파일 내용이 바이트까지 같다는 뜻이다. 변수별 해시는 무엇이 다른지 알려 주는 데만 쓴다.

use std::collections::BTreeMap;

use crate::port::ProgressSink;

use super::env_file::EnvFileRole;
use super::server_link::{ServerSeat, ServerSeats, find_environment, seat_of};
use super::{ProjectError, ProjectStore, Workspace};

/// 파일 하나를 해시로 읽은 것. 값은 없다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvDigest {
    /// 파일 전체의 해시.
    pub file: String,
    /// 변수 이름 → 값의 해시. 같은 이름이 여러 번 나오면 마지막 것이다.
    pub keys: BTreeMap<String, String>,
}

/// 서버 레포가 그 파일을 어떻게 다루는가.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoTracking {
    /// `.gitignore` 로 제외된다.
    Ignored,
    /// 제외 규칙이 없다. `git status` 에 나오고 실수로 커밋될 수 있다.
    Unignored,
    /// 레포가 추적한다. 코드를 받을 때 덮어쓰이거나 충돌한다.
    Tracked,
    /// 배포 경로가 git 저장소가 아니다.
    NoRepository,
}

/// 서버의 그 파일을 읽은 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEnv {
    /// 배포 경로가 없다. 코드를 먼저 받아야 한다.
    NoDirectory,
    Missing {
        tracking: RepoTracking,
    },
    Present {
        digest: EnvDigest,
        /// 파일 권한(8진수)과 소유자.
        mode: Option<String>,
        owner: Option<String>,
        tracking: RepoTracking,
    },
}

/// 로컬 뿌리의 환경 변수 파일.
pub trait LocalEnvFiles: Send + Sync {
    /// 비교 한 번에 쓸 임의 값.
    fn salt(&self) -> String;
    fn digest(&self, project_path: &str, file: &str, salt: &str)
    -> Result<EnvDigest, ProjectError>;
    /// 서버에 올릴 내용. 올리는 데만 쓰고 어디에도 남기지 않는다.
    fn read(&self, project_path: &str, file: &str) -> Result<Vec<u8>, ProjectError>;
}

/// 서버 배포 경로 뿌리의 환경 변수 파일.
pub trait ServerEnvFiles: Send + Sync {
    fn digest(
        &self,
        seat: &ServerSeat,
        dir: &str,
        file: &str,
        salt: &str,
        progress: &dyn ProgressSink,
    ) -> Result<ServerEnv, ProjectError>;
    /// `<dir>/<file>` 을 이 내용으로 바꾼다. 권한은 0600 이다. 배포 경로가 없으면 실패한다.
    fn write(
        &self,
        seat: &ServerSeat,
        dir: &str,
        file: &str,
        contents: &[u8],
        progress: &dyn ProgressSink,
    ) -> Result<(), ProjectError>;
}

/// 로컬 파일과 서버 파일이 어떤 관계인가.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvState {
    NoDirectory,
    ServerMissing,
    Same,
    Differ {
        /// 로컬에만 있는 변수. 올리면 서버에 생긴다.
        local_only: Vec<String>,
        /// 서버에만 있는 변수. 올리면 서버에서 사라진다.
        server_only: Vec<String>,
        /// 양쪽에 있지만 값이 다른 변수.
        changed: Vec<String>,
    },
}

impl EnvState {
    pub fn same(&self) -> bool {
        *self == EnvState::Same
    }
}

#[derive(Debug, Clone)]
pub struct EnvComparison {
    /// 로컬 파일 이름.
    pub local_file: String,
    /// 서버 파일의 절대 경로.
    pub server_file: String,
    pub state: EnvState,
    pub mode: Option<String>,
    pub owner: Option<String>,
    /// 배포 경로가 없으면 없다.
    pub tracking: Option<RepoTracking>,
}

/// 두 해시 묶음의 차이.
pub fn compare(local: &EnvDigest, server: &EnvDigest) -> EnvState {
    if local.file == server.file {
        return EnvState::Same;
    }
    let local_only = local
        .keys
        .keys()
        .filter(|k| !server.keys.contains_key(*k))
        .cloned()
        .collect();
    let server_only = server
        .keys
        .keys()
        .filter(|k| !local.keys.contains_key(*k))
        .cloned()
        .collect();
    let changed = local
        .keys
        .iter()
        .filter(|(k, v)| server.keys.get(*k).is_some_and(|s| s != *v))
        .map(|(k, _)| k.clone())
        .collect();
    EnvState::Differ {
        local_only,
        server_only,
        changed,
    }
}

pub struct EnvSync<'a> {
    store: &'a dyn ProjectStore,
    workspace: &'a dyn Workspace,
    seats: &'a dyn ServerSeats,
    local: &'a dyn LocalEnvFiles,
    server: &'a dyn ServerEnvFiles,
}

impl<'a> EnvSync<'a> {
    pub fn new(
        store: &'a dyn ProjectStore,
        workspace: &'a dyn Workspace,
        seats: &'a dyn ServerSeats,
        local: &'a dyn LocalEnvFiles,
        server: &'a dyn ServerEnvFiles,
    ) -> EnvSync<'a> {
        EnvSync {
            store,
            workspace,
            seats,
            local,
            server,
        }
    }

    /// 이 환경에 올릴 로컬 파일과 서버에 둘 이름을 정한다. `file` 이 `None` 이면 고른 것을 지운다.
    ///
    /// 뿌리에 있는 환경 변수 파일만 고를 수 있다. 예시 파일은 값이 없어야 하므로 고를 수 없다.
    /// 서버 이름은 뿌리의 파일 이름 하나다(디렉토리 없음).
    pub fn choose(
        &self,
        name: &str,
        environment: &str,
        file: Option<&str>,
        server_file: &str,
    ) -> Result<(), ProjectError> {
        let server_file = check_server_file(server_file)?;
        let mut record = self.store.load(name)?;
        let chosen = match file {
            None => None,
            Some(file) => {
                let scan = self.workspace.scan(&record.path)?;
                let view = scan
                    .env_view()
                    .into_iter()
                    .find(|f| f.name == file)
                    .ok_or_else(|| {
                        ProjectError::Missing(format!("{} 뿌리의 {file}", record.path))
                    })?;
                if view.role == EnvFileRole::Example {
                    return Err(ProjectError::Invalid(format!(
                        "{file}은(는) 예시 파일이라 서버에 올릴 수 없습니다."
                    )));
                }
                Some(view.name)
            }
        };
        let env = record
            .environments
            .iter_mut()
            .find(|e| e.name == environment)
            .ok_or_else(|| ProjectError::Missing(format!("환경 {environment}")))?;
        env.env_file = chosen;
        env.server_env_file = server_file;
        self.store.replace(&record)
    }

    /// 로컬 파일과 서버 파일을 해시로 비교한다. 아무것도 바꾸지 않는다.
    pub fn compare(
        &self,
        name: &str,
        environment: &str,
        progress: &dyn ProgressSink,
    ) -> Result<EnvComparison, ProjectError> {
        let target = self.target(name, environment)?;
        self.compare_target(&target, progress)
    }

    /// 로컬 파일을 서버 파일로 올리고, 다시 비교해 같아졌는지 확인한다.
    pub fn push(
        &self,
        name: &str,
        environment: &str,
        progress: &dyn ProgressSink,
    ) -> Result<EnvComparison, ProjectError> {
        let target = self.target(name, environment)?;
        let contents = self.local.read(&target.project_path, &target.file)?;
        self.server.write(
            &target.seat,
            &target.dir,
            &target.server_file,
            &contents,
            progress,
        )?;
        let after = self.compare_target(&target, progress)?;
        if !after.state.same() {
            return Err(ProjectError::Storage(format!(
                "{}에 올렸지만 다시 비교했을 때 로컬 {}과(와) 같지 않습니다. 작업 로그를 확인하세요.",
                after.server_file, target.file
            )));
        }
        Ok(after)
    }

    fn target(&self, name: &str, environment: &str) -> Result<Target, ProjectError> {
        let record = self.store.load(name)?;
        let env = find_environment(&record.environments, environment)?;
        let file = env.env_file.clone().ok_or_else(|| {
            ProjectError::Invalid(format!(
                "환경 {environment}에 올릴 로컬 파일을 먼저 고르세요."
            ))
        })?;
        let seat = seat_of(self.seats, env)?;
        Ok(Target {
            project_path: record.path.clone(),
            file,
            server_file: env.server_env_file.clone(),
            seat,
            dir: env.path.clone(),
        })
    }

    fn compare_target(
        &self,
        target: &Target,
        progress: &dyn ProgressSink,
    ) -> Result<EnvComparison, ProjectError> {
        let salt = self.local.salt();
        let local = self
            .local
            .digest(&target.project_path, &target.file, &salt)?;
        let server = self.server.digest(
            &target.seat,
            &target.dir,
            &target.server_file,
            &salt,
            progress,
        )?;
        let server_file = format!(
            "{}/{}",
            target.dir.trim_end_matches('/'),
            target.server_file
        );
        let (state, mode, owner, tracking) = match server {
            ServerEnv::NoDirectory => (EnvState::NoDirectory, None, None, None),
            ServerEnv::Missing { tracking } => {
                (EnvState::ServerMissing, None, None, Some(tracking))
            }
            ServerEnv::Present {
                digest,
                mode,
                owner,
                tracking,
            } => (compare(&local, &digest), mode, owner, Some(tracking)),
        };
        Ok(EnvComparison {
            local_file: target.file.clone(),
            server_file,
            state,
            mode,
            owner,
            tracking,
        })
    }
}

struct Target {
    project_path: String,
    file: String,
    server_file: String,
    seat: ServerSeat,
    dir: String,
}

/// 서버 배포 경로 뿌리에 둘 파일 이름.
pub fn check_server_file(text: &str) -> Result<String, ProjectError> {
    let name = text.trim();
    let valid = !name.is_empty()
        && name.len() <= 64
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if valid {
        Ok(name.to_string())
    } else {
        Err(ProjectError::Invalid(
            "서버 파일 이름은 배포 경로 뿌리의 파일 이름 하나입니다. 영문 · 숫자 · . · - · _만 쓸 수 있습니다.".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(file: &str, keys: &[(&str, &str)]) -> EnvDigest {
        EnvDigest {
            file: file.into(),
            keys: keys
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    mod compare {
        use super::*;

        #[test]
        fn the_same_file_is_the_same_whatever_the_keys_say() {
            assert_eq!(
                compare(&digest("f", &[("A", "1")]), &digest("f", &[("A", "1")])),
                EnvState::Same
            );
        }

        #[test]
        fn names_what_would_appear_disappear_and_change() {
            let local = digest("l", &[("A", "1"), ("B", "2"), ("C", "3")]);
            let server = digest("s", &[("B", "2"), ("C", "x"), ("D", "4")]);
            assert_eq!(
                compare(&local, &server),
                EnvState::Differ {
                    local_only: vec!["A".into()],
                    server_only: vec!["D".into()],
                    changed: vec!["C".into()],
                }
            );
        }

        #[test]
        fn a_different_file_with_the_same_variables_still_differs() {
            let state = compare(&digest("l", &[("A", "1")]), &digest("s", &[("A", "1")]));
            assert_eq!(
                state,
                EnvState::Differ {
                    local_only: vec![],
                    server_only: vec![],
                    changed: vec![],
                }
            );
        }
    }
}
