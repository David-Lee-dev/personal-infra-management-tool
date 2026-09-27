//! 코드 받기 — 연결된 환경의 배포 경로에 그 환경의 브랜치를 clone 한다.
//!
//! 이 도구가 서버에 쓰는 첫 절차다. 그래서 쓰기 전과 후에 서버를 읽어 확인한다.
//! 순서: 환경 · 계정 찾기 → 배포 경로 읽기 → 키(사용자가 고른 저장된 키) → 서버에 키를 두고 clone
//! → 다시 읽어 같은 레포인지 확인. 키는 자격 증명 화면에서 발급한 것만 쓴다 — 여기서 만들지 않는다.
//!
//! 배포 경로에 이미 같은 레포가 있으면 아무것도 하지 않는다. 다른 레포나 git 이 아닌 파일이
//! 있으면 건드리지 않고 멈춘다 — 서버의 파일을 지우거나 덮어쓰지 않는다.

use crate::key::RepoRef;
use crate::port::ProgressSink;

use super::git_link::{RepoKey, RepoKeys};
use super::server_link::{
    Checkout, ServerProbe, ServerSeat, ServerSeats, find_environment, github_repo, same_repository,
    seat_of,
};
use super::{ProjectError, ProjectStore, Workspace};

/// 서버에 쓰는 곳.
pub trait ServerCode: Send + Sync {
    /// 서버 계정에 레포 키를 두고(`~/.ssh/github/<레포>`) 배포 경로로 그 브랜치를 clone 한 뒤,
    /// 그 레포의 `core.sshCommand` 가 그 키를 쓰게 한다.
    ///
    /// `private_key` 는 이 머신의 개인 키 파일 경로다. 값은 구현이 읽어 stdin 으로만 넘긴다.
    fn clone_repository(
        &self,
        seat: &ServerSeat,
        repo: &RepoRef,
        branch: &str,
        path: &str,
        private_key: &str,
        progress: &dyn ProgressSink,
    ) -> Result<(), ProjectError>;
}

#[derive(Debug)]
pub struct Pulled {
    pub checkout: Checkout,
    /// 이미 받아 둔 상태였다. 아무것도 하지 않았다.
    pub already: bool,
    /// 쓴 키. 이미 받아 둔 상태였으면 없다.
    pub key: Option<RepoKey>,
}

pub struct CodePull<'a> {
    store: &'a dyn ProjectStore,
    workspace: &'a dyn Workspace,
    seats: &'a dyn ServerSeats,
    probe: &'a dyn ServerProbe,
    keys: &'a dyn RepoKeys,
    code: &'a dyn ServerCode,
}

impl<'a> CodePull<'a> {
    pub fn new(
        store: &'a dyn ProjectStore,
        workspace: &'a dyn Workspace,
        seats: &'a dyn ServerSeats,
        probe: &'a dyn ServerProbe,
        keys: &'a dyn RepoKeys,
        code: &'a dyn ServerCode,
    ) -> CodePull<'a> {
        CodePull {
            store,
            workspace,
            seats,
            probe,
            keys,
            code,
        }
    }

    /// 이 프로젝트 레포와 그 레포의 저장된 키 전부. 서버에 둘 키를 고르게 보여 준다. 읽기만 한다.
    pub fn keys_of(&self, name: &str) -> Result<(RepoRef, Vec<RepoKey>), ProjectError> {
        let record = self.store.load(name)?;
        let repo = github_repo(self.workspace, &record.path)?;
        let keys = self.keys.keys_for(&repo);
        Ok((repo, keys))
    }

    /// `key` 는 서버에 둘 저장된 키의 용도다.
    pub fn pull(
        &self,
        name: &str,
        environment: &str,
        key: &str,
        progress: &dyn ProgressSink,
    ) -> Result<Pulled, ProjectError> {
        let record = self.store.load(name)?;
        let env = find_environment(&record.environments, environment)?;
        let repo = github_repo(self.workspace, &record.path)?;
        let seat = seat_of(self.seats, env)?;

        let before = self.probe.checkout(&seat, &env.path, progress)?;
        match &before {
            Checkout::Missing | Checkout::Empty => {}
            Checkout::Repository {
                origin: Some(origin),
                ..
            } if same_repository(origin, &repo) => {
                return Ok(Pulled {
                    checkout: before,
                    already: true,
                    key: None,
                });
            }
            Checkout::Repository { origin, .. } => {
                return Err(ProjectError::Invalid(format!(
                    "{}에 다른 레포({})가 있습니다. 서버의 파일은 건드리지 않았습니다.",
                    env.path,
                    origin.as_deref().unwrap_or("origin 없음")
                )));
            }
            Checkout::Plain => {
                return Err(ProjectError::Invalid(format!(
                    "{}에 git 저장소가 아닌 파일이 있습니다. 서버의 파일은 건드리지 않았습니다.",
                    env.path
                )));
            }
        }

        let key = self.server_key(&repo, key)?;
        self.code.clone_repository(
            &seat,
            &repo,
            &env.branch,
            &env.path,
            &key.private_key,
            progress,
        )?;

        let after = self.probe.checkout(&seat, &env.path, progress)?;
        let landed = matches!(&after, Checkout::Repository { origin: Some(o), .. } if same_repository(o, &repo));
        if !landed {
            return Err(ProjectError::Storage(format!(
                "clone은 끝났지만 {}에서 {}을(를) 확인하지 못했습니다. 작업 로그를 확인하세요.",
                env.path,
                repo.slug()
            )));
        }
        Ok(Pulled {
            checkout: after,
            already: false,
            key: Some(key),
        })
    }

    /// 고른 저장된 키. GitHub 등록까지 끝난 것이어야 한다.
    fn server_key(&self, repo: &RepoRef, purpose: &str) -> Result<RepoKey, ProjectError> {
        let key = self
            .keys
            .keys_for(repo)
            .into_iter()
            .find(|k| k.purpose == purpose)
            .ok_or_else(|| ProjectError::Missing(format!("{} 레포의 {purpose} 키", repo.slug())))?;
        if !key.usable {
            return Err(ProjectError::Invalid(format!(
                "{purpose} 키는 GitHub 등록이 끝나지 않았습니다. 자격 증명 화면에서 등록을 다시 시도하세요."
            )));
        }
        Ok(key)
    }
}
