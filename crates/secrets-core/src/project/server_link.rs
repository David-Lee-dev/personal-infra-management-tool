//! 서버 연결 — 이미 준비된 서버 계정을 프로젝트의 환경으로 잇는다.
//!
//! 인스턴스와 계정은 사용자가 만든다(AWS 콘솔 · 자격 증명 화면). 여기서는 인스턴스와 그
//! 인스턴스의 확인된 배포 계정을 골라 환경 이름을 붙인다. 배포 경로는 규칙(`/srv/<레포 이름>`)
//! 으로 정해지고, 그 계정의 키로 서버에 들어가 그 경로가 맞는지 읽어 본다.
//! 서버에 쓰지 않는다 — 코드를 받거나 `.env` 를 반영하는 일은 뒤의 단계다.

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
    /// 배포에 쓰는 서버 계정.
    pub login: String,
    /// 서버의 배포 경로. 절대 경로.
    pub path: String,
    pub connected_at: String,
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
    /// sudo 가 있는 관리 계정인가.
    pub admin: bool,
    /// 그 키로 실제로 들어가 봤다.
    pub verified: bool,
}

impl ServerSeat {
    pub fn slug(&self) -> String {
        format!("{}/{}", self.instance, self.login)
    }
}

/// 서버의 배포 경로를 읽은 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checkout {
    /// 경로가 없다. 코드는 아직 받지 않았다.
    Missing,
    /// 빈 디렉토리다. 코드는 아직 받지 않았다.
    Empty,
    /// 파일이 있지만 git 저장소가 아니다.
    Plain,
    Repository {
        origin: Option<String>,
        branch: Option<String>,
        commit: Option<String>,
    },
}

/// 서버 계정들이 놓인 곳.
pub trait ServerSeats: Send + Sync {
    fn seats(&self) -> Vec<ServerSeat>;
}

/// 서버를 읽는다. 쓰지 않는다.
pub trait ServerProbe: Send + Sync {
    /// 그 계정의 키로 들어가 배포 경로를 본다.
    fn checkout(&self, seat: &ServerSeat, path: &str, progress: &dyn ProgressSink)
    -> Result<Checkout, ProjectError>;
}

#[derive(Debug, Clone)]
pub struct ServerRequest {
    pub environment: String,
    /// `ServerSeat::slug` — `인스턴스/계정`.
    pub seat: String,
}

/// 서버의 배포 경로 규칙 — `/srv/<레포 이름>`. 사람이 고르지 않는다.
pub fn deploy_path(repo: &RepoRef) -> String {
    format!("/srv/{}", repo.name())
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

    /// 이 프로젝트의 배포 경로. Git 이 먼저 연결돼 있어야 정해진다.
    pub fn deploy_path_of(&self, name: &str) -> Result<String, ProjectError> {
        let record = self.store.load(name)?;
        Ok(deploy_path(&self.project_repo(&record.path)?))
    }

    /// 고를 수 있는 서버 계정들.
    pub fn seats(&self) -> Vec<ServerSeat> {
        self.seats.seats()
    }

    /// 이미 붙은 환경의 배포 경로를 지금 읽는다.
    pub fn check(
        &self,
        name: &str,
        environment: &str,
        progress: &dyn ProgressSink,
    ) -> Result<Checkout, ProjectError> {
        let record = self.store.load(name)?;
        let env = record
            .environments
            .iter()
            .find(|e| e.name == environment)
            .ok_or_else(|| ProjectError::Missing(format!("환경 {environment}")))?;
        let seat = self
            .seats
            .seats()
            .into_iter()
            .find(|s| s.instance == env.instance && s.login == env.login)
            .ok_or_else(|| ProjectError::Missing(format!("서버 계정 {}/{}", env.instance, env.login)))?;
        self.probe.checkout(&seat, &env.path, progress)
    }

    /// 서버 계정을 환경으로 잇는다.
    ///
    /// 순서: 값 검사 → 프로젝트 Git 확인(배포 경로가 여기서 정해진다) → 계정 검사 → 서버 읽기 →
    /// 기록. 서버에서 읽은 레포가 이 프로젝트의 레포와 다르면 기록하지 않는다.
    pub fn attach(
        &self,
        name: &str,
        request: &ServerRequest,
        progress: &dyn ProgressSink,
    ) -> Result<Attached, ProjectError> {
        let environment = check_environment(&request.environment)?;

        let mut record = self.store.load(name)?;
        if record.environments.iter().any(|e| e.name == environment) {
            return Err(ProjectError::Invalid(format!("환경 {environment}은(는) 이미 연결되어 있습니다.")));
        }
        let repo = self.project_repo(&record.path)?;
        let path = deploy_path(&repo);

        let seat = self
            .seats
            .seats()
            .into_iter()
            .find(|s| s.slug() == request.seat)
            .ok_or_else(|| ProjectError::Missing(format!("서버 계정 {}", request.seat)))?;
        if seat.admin {
            return Err(ProjectError::Invalid(
                "관리 계정(sudo)으로는 배포하지 않습니다. 배포 계정을 고르세요.".into(),
            ));
        }
        if !seat.verified {
            return Err(ProjectError::Invalid(format!(
                "{}은(는) 아직 접속 확인이 끝나지 않았습니다. 자격 증명 화면에서 확인한 뒤 연결하세요.",
                seat.slug()
            )));
        }

        let checkout = self.probe.checkout(&seat, &path, progress)?;
        if let Checkout::Repository { origin: Some(origin), .. } = &checkout
            && !same_repository(origin, &repo)
        {
            return Err(ProjectError::Invalid(format!(
                "{path}에는 다른 레포({origin})가 있습니다. 이 프로젝트의 레포는 {}입니다.",
                repo.slug()
            )));
        }
        if checkout == Checkout::Plain {
            return Err(ProjectError::Invalid(format!(
                "{path}에 git 저장소가 아닌 파일이 있습니다. 배포 경로는 /srv/<레포 이름> 규칙을 따르므로, 서버에서 이 디렉토리를 정리하거나 옮긴 뒤 다시 연결하세요."
            )));
        }

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
            connected_at: self.clock.now(),
        };
        record.environments.push(env.clone());
        self.store.replace(&record)?;
        Ok(Attached {
            environment: env,
            checkout,
        })
    }

    /// 서버는 GitHub 에서 코드를 받는다. 그래서 GitHub origin 이 먼저 있어야 한다.
    fn project_repo(&self, path: &str) -> Result<RepoRef, ProjectError> {
        let scan = self.workspace.scan(path)?;
        match &scan.git {
            GitState::Remote { origin, .. } => RepoRef::parse(origin).ok_or_else(|| {
                ProjectError::Invalid(format!("origin({origin})이 GitHub 레포가 아닙니다."))
            }),
            GitState::Local { .. } | GitState::Absent => {
                Err(ProjectError::Invalid("Git을 먼저 연결하세요. 서버는 GitHub에서 코드를 받습니다.".into()))
            }
        }
    }
}

fn same_repository(origin: &str, repo: &RepoRef) -> bool {
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
