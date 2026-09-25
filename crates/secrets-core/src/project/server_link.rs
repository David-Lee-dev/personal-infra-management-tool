//! 서버 연결 — 인스턴스의 서버 계정 하나를 프로젝트의 환경으로 잇는다.
//!
//! 무엇으로 잇는지는 사용자가 정한다 — 인스턴스, 계정, 배포 경로, 브랜치. 이 절차는 고른 값을
//! 기록하고, 그 계정의 키로 서버에 들어가 배포 경로에 무엇이 있는지 읽어 보여 줄 뿐이다.
//! 서버에 쓰지 않는다.

use serde::{Deserialize, Serialize};

use crate::key::RepoRef;
use crate::port::{Clock, ProgressSink};

use super::scan::GitState;
use super::{ProjectError, ProjectStore, Workspace};

/// 프로젝트가 붙은 서버 환경 하나. `project.toml` 의 `[[environments]]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    pub name: String,
    pub aws_account: String,
    /// ec2 | lightsail
    pub machine: String,
    pub region: String,
    pub keypair: String,
    pub instance: String,
    #[serde(default)]
    pub instance_name: String,
    pub address: String,
    /// 이 환경에 쓰는 서버 계정.
    pub login: String,
    /// 서버의 배포 경로. 절대 경로.
    pub path: String,
    /// 이 환경이 배포하는 브랜치.
    #[serde(default)]
    pub branch: String,
    pub connected_at: String,
    /// 이 환경의 서버 `.env` 로 올리는 로컬 뿌리의 파일. 사용자가 고른다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_file: Option<String>,
    /// 서버 배포 경로 뿌리에서 그 파일을 둘 이름. 런타임이 읽는 이름을 사용자가 고른다.
    #[serde(default = "default_server_env_file")]
    pub server_env_file: String,
}

pub fn default_server_env_file() -> String {
    ".env".into()
}

/// 시크릿 저장소에 있는 서버 계정 하나. 값은 없고 자리만 있다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSeat {
    pub aws_account: String,
    pub machine: String,
    pub region: String,
    pub keypair: String,
    pub instance: String,
    pub instance_name: String,
    pub address: String,
    pub login: String,
    /// sudo 가 있는 계정인가.
    pub admin: bool,
    /// 그 키로 실제로 들어가 봤다.
    pub verified: bool,
    /// 이 계정으로 들어가는 개인 키 파일의 자리. 값이 아니라 경로다.
    pub key_path: String,
}

impl ServerSeat {
    pub fn slug(&self) -> String {
        format!("{}/{}", self.instance, self.login)
    }
}

/// 인스턴스 하나와 그 위의 서버 계정들.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInstance {
    pub instance: String,
    pub name: String,
    pub address: String,
    pub machine: String,
    pub accounts: Vec<ServerSeat>,
}

/// 배포 경로에서 읽은 사실.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckoutFacts {
    pub owner: Option<String>,
    pub group: Option<String>,
    pub ssh_command: Option<String>,
}

/// 서버의 배포 경로를 읽은 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checkout {
    /// 경로가 없다.
    Missing,
    /// 빈 디렉토리다.
    Empty,
    /// 파일이 있지만 git 저장소가 아니다.
    Plain,
    Repository {
        origin: Option<String>,
        branch: Option<String>,
        commit: Option<String>,
        facts: CheckoutFacts,
    },
}

/// 서버 계정들이 놓인 곳.
pub trait ServerSeats: Send + Sync {
    fn seats(&self) -> Vec<ServerSeat>;
}

/// 서버를 읽는다. 쓰지 않는다.
pub trait ServerProbe: Send + Sync {
    /// 그 계정의 키로 들어가 배포 경로를 본다.
    fn checkout(
        &self,
        seat: &ServerSeat,
        path: &str,
        progress: &dyn ProgressSink,
    ) -> Result<Checkout, ProjectError>;
}

#[derive(Debug, Clone)]
pub struct ServerRequest {
    pub environment: String,
    pub instance: String,
    pub login: String,
    pub path: String,
    pub branch: String,
}

#[derive(Debug)]
pub struct Attached {
    pub environment: Environment,
    pub checkout: Checkout,
}

/// 환경 이름. 로컬 파일 `.env.<환경>` 의 이름이 되므로 파일 이름으로 안전한 글자만 받는다.
pub fn check_environment(text: &str) -> Result<String, ProjectError> {
    let name = text.trim();
    let valid = !name.is_empty()
        && name.len() <= 24
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if valid && name != "local" && name != "example" {
        Ok(name.to_string())
    } else {
        Err(ProjectError::Invalid(
            "환경 이름은 영문 소문자로 시작하고 소문자 · 숫자 · - · _만 쓸 수 있습니다. local과 example은 쓸 수 없습니다."
                .into(),
        ))
    }
}

pub struct ServerLink<'a> {
    store: &'a dyn ProjectStore,
    workspace: &'a dyn Workspace,
    seats: &'a dyn ServerSeats,
    probe: &'a dyn ServerProbe,
    clock: &'a dyn Clock,
}

impl<'a> ServerLink<'a> {
    pub fn new(
        store: &'a dyn ProjectStore,
        workspace: &'a dyn Workspace,
        seats: &'a dyn ServerSeats,
        probe: &'a dyn ServerProbe,
        clock: &'a dyn Clock,
    ) -> ServerLink<'a> {
        ServerLink {
            store,
            workspace,
            seats,
            probe,
            clock,
        }
    }

    /// 이 프로젝트의 GitHub 레포. 서버에 이을 때 레포 이름을 보여 주는 데 쓴다.
    pub fn repo_of(&self, name: &str) -> Result<RepoRef, ProjectError> {
        let record = self.store.load(name)?;
        github_repo(self.workspace, &record.path)
    }

    /// 인스턴스마다 그 위의 서버 계정.
    pub fn instances(&self) -> Vec<ServerInstance> {
        let mut instances: Vec<ServerInstance> = Vec::new();
        for seat in self.seats.seats() {
            match instances.iter_mut().find(|i| i.instance == seat.instance) {
                Some(found) => found.accounts.push(seat),
                None => instances.push(ServerInstance {
                    instance: seat.instance.clone(),
                    name: seat.instance_name.clone(),
                    address: seat.address.clone(),
                    machine: seat.machine.clone(),
                    accounts: vec![seat],
                }),
            }
        }
        for instance in &mut instances {
            instance.accounts.sort_by(|a, b| a.login.cmp(&b.login));
        }
        instances.sort_by(|a, b| (&a.name, &a.instance).cmp(&(&b.name, &b.instance)));
        instances
    }

    /// 붙은 환경의 배포 경로를 지금 읽는다. 아무것도 바꾸지 않는다.
    pub fn check(
        &self,
        name: &str,
        environment: &str,
        progress: &dyn ProgressSink,
    ) -> Result<Checkout, ProjectError> {
        let record = self.store.load(name)?;
        let env = find_environment(&record.environments, environment)?;
        let seat = seat_of(self.seats, &env.instance, &env.login)?;
        self.probe.checkout(&seat, &env.path, progress)
    }

    /// 고른 서버 계정을 환경으로 잇는다.
    ///
    /// 순서: 값 검사 → 프로젝트 Git 확인 → 계정 찾기 → 서버 읽기 → 기록. 배포 경로에 다른 레포나
    /// git 이 아닌 파일이 있으면 그 사실을 알리고 기록하지 않는다.
    pub fn attach(
        &self,
        name: &str,
        request: &ServerRequest,
        progress: &dyn ProgressSink,
    ) -> Result<Attached, ProjectError> {
        let environment = check_environment(&request.environment)?;
        let path = request.path.trim().trim_end_matches('/').to_string();
        if !path.starts_with('/') || path.len() < 2 {
            return Err(ProjectError::Invalid(
                "배포 경로는 /로 시작하는 절대 경로여야 합니다.".into(),
            ));
        }
        let branch = request.branch.trim().to_string();
        if branch.is_empty() {
            return Err(ProjectError::Invalid("배포할 브랜치를 입력하세요.".into()));
        }

        let mut record = self.store.load(name)?;
        if record.environments.iter().any(|e| e.name == environment) {
            return Err(ProjectError::Invalid(format!(
                "환경 {environment}은(는) 이미 연결되어 있습니다."
            )));
        }
        let repo = github_repo(self.workspace, &record.path)?;
        let seat = seat_of(self.seats, &request.instance, &request.login)?;

        let checkout = self.probe.checkout(&seat, &path, progress)?;
        accept_checkout(&checkout, &repo, &path)?;

        let env = Environment {
            name: environment,
            aws_account: seat.aws_account,
            machine: seat.machine,
            region: seat.region,
            keypair: seat.keypair,
            instance: seat.instance,
            instance_name: seat.instance_name,
            address: seat.address,
            login: seat.login,
            path,
            branch,
            connected_at: self.clock.now(),
            env_file: None,
            server_env_file: default_server_env_file(),
        };
        record.environments.push(env.clone());
        self.store.replace(&record)?;
        Ok(Attached {
            environment: env,
            checkout,
        })
    }
}

/// 배포 경로로 쓸 수 있는가 — 없거나, 비었거나, 이 레포를 받아 둔 경로여야 한다.
pub(super) fn accept_checkout(
    checkout: &Checkout,
    repo: &RepoRef,
    path: &str,
) -> Result<(), ProjectError> {
    if let Checkout::Repository {
        origin: Some(origin),
        ..
    } = checkout
        && !same_repository(origin, repo)
    {
        return Err(ProjectError::Invalid(format!(
            "{path}에는 다른 레포({origin})가 있습니다. 이 프로젝트의 레포는 {}입니다.",
            repo.slug()
        )));
    }
    if *checkout == Checkout::Plain {
        return Err(ProjectError::Invalid(format!(
            "{path}에 git 저장소가 아닌 파일이 있습니다. 비어 있는 경로나 이 레포를 받아 둔 경로를 입력하세요."
        )));
    }
    Ok(())
}

pub(super) fn find_environment<'r>(
    environments: &'r [Environment],
    name: &str,
) -> Result<&'r Environment, ProjectError> {
    environments
        .iter()
        .find(|e| e.name == name)
        .ok_or_else(|| ProjectError::Missing(format!("환경 {name}")))
}

/// 서버는 GitHub 에서 코드를 받는다. 그래서 프로젝트에 GitHub origin 이 먼저 있어야 한다.
pub(super) fn github_repo(workspace: &dyn Workspace, path: &str) -> Result<RepoRef, ProjectError> {
    let scan = workspace.scan(path)?;
    match &scan.git {
        GitState::Remote { origin, .. } => RepoRef::parse(origin).ok_or_else(|| {
            ProjectError::Invalid(format!("origin({origin})이 GitHub 레포가 아닙니다."))
        }),
        GitState::Local { .. } | GitState::Absent => Err(ProjectError::Invalid(
            "Git을 먼저 연결하세요. 서버는 GitHub에서 코드를 받습니다.".into(),
        )),
    }
}

/// 시크릿 저장소에 있는 그 인스턴스의 그 계정.
pub(super) fn seat_of(
    seats: &dyn ServerSeats,
    instance: &str,
    login: &str,
) -> Result<ServerSeat, ProjectError> {
    seats
        .seats()
        .into_iter()
        .find(|s| s.instance == instance && s.login == login)
        .ok_or_else(|| ProjectError::Missing(format!("서버 계정 {instance}/{login}")))
}

pub(super) fn same_repository(origin: &str, repo: &RepoRef) -> bool {
    RepoRef::parse(origin).is_some_and(|r| r.slug().eq_ignore_ascii_case(&repo.slug()))
}

#[cfg(test)]
mod tests {
    use super::*;

    mod check_environment {
        use super::*;

        #[test]
        fn accepts_plain_lowercase_names() {
            for name in ["prod", "dev", "staging-2", "qa_1"] {
                assert_eq!(check_environment(name).unwrap(), name);
            }
        }

        #[test]
        fn rejects_reserved_uppercase_and_path_like_names() {
            for bad in ["", "local", "example", "Prod", "1prod", "a/b", "a.b"] {
                assert!(check_environment(bad).is_err(), "{bad}");
            }
        }
    }
}
