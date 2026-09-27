//! 등록한 뒤의 수정과 제거 — 프로젝트와 환경의 **기록만** 고친다.
//!
//! 로컬 디렉토리 · 서버 · GitHub 레포 · 키는 건드리지 않는다. 빼는 것은 지우지 않고 보관소로
//! 옮긴다. 환경의 서버 쪽을 바꿀 때는 처음 연결할 때와 같은 검사를 거친다.
//!
//! ```text
//! 프로젝트 이름 바꾸기   projects/<옛 이름>/ → projects/<새 이름>/   (배포 스크립트까지 함께)
//! 프로젝트 등록 해제     projects/<이름>/ → archive/projects/<시각>-<이름>/
//! 환경 이름 바꾸기       projects/<이름>/deploy/<옛 환경>/ → deploy/<새 환경>/
//! 환경 빼기              기록과 deploy/<환경>/ → archive/projects/<이름>/environments/<시각>-<환경>/
//! ```

use crate::port::ProgressSink;

use super::naming;
use super::server_link::{
    Checkout, Environment, ServerProbe, ServerSeats, accept_checkout, check_environment,
    find_environment, github_repo, seat_by,
};
use super::{PathState, ProjectError, ProjectRecord, ProjectStore, Workspace};

/// 프로젝트 기록의 디렉토리를 옮긴다.
pub trait ProjectFiles: Send + Sync {
    /// `projects/<from>/` 을 `projects/<to>/` 로. `to` 가 있으면 실패한다.
    fn rename_project(&self, from: &str, to: &str) -> Result<(), ProjectError>;
    /// `projects/<name>/` 을 보관소로. 옮긴 자리를 돌려준다.
    fn archive_project(&self, name: &str) -> Result<String, ProjectError>;
    /// `deploy/<from>/` 을 `deploy/<to>/` 로. 없으면 아무것도 하지 않는다.
    fn rename_environment(&self, project: &str, from: &str, to: &str) -> Result<(), ProjectError>;
    /// 환경의 기록(`record`, toml)과 `deploy/<환경>/` 을 보관소로. 옮긴 자리를 돌려준다.
    fn archive_environment(
        &self,
        project: &str,
        environment: &str,
        record: &str,
    ) -> Result<String, ProjectError>;
}

#[derive(Debug, Clone)]
pub struct ProjectEdit {
    pub name: String,
    pub group: String,
    /// 절대 경로.
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct EnvironmentEdit {
    pub name: String,
    pub server: String,
    pub login: String,
    pub path: String,
    pub branch: String,
}

#[derive(Debug)]
pub struct EditedEnvironment {
    pub environment: Environment,
    /// 서버 쪽을 바꿔 다시 읽었으면 그 결과.
    pub checkout: Option<Checkout>,
}

pub struct ProjectEditor<'a> {
    store: &'a dyn ProjectStore,
    files: &'a dyn ProjectFiles,
    workspace: &'a dyn Workspace,
    seats: &'a dyn ServerSeats,
    probe: &'a dyn ServerProbe,
}

impl<'a> ProjectEditor<'a> {
    pub fn new(
        store: &'a dyn ProjectStore,
        files: &'a dyn ProjectFiles,
        workspace: &'a dyn Workspace,
        seats: &'a dyn ServerSeats,
        probe: &'a dyn ServerProbe,
    ) -> ProjectEditor<'a> {
        ProjectEditor {
            store,
            files,
            workspace,
            seats,
            probe,
        }
    }

    /// 이름 · 그룹 · 경로를 바꾼다. 바뀐 기록을 돌려준다.
    ///
    /// 경로는 이미 있는 디렉토리여야 한다 — 디렉토리를 옮긴 뒤 기록을 맞추는 일이다.
    pub fn update(&self, name: &str, edit: &ProjectEdit) -> Result<ProjectRecord, ProjectError> {
        let mut record = self.store.load(name)?;
        let new_name = naming::check_name(&edit.name)?;
        let group = naming::check_group(&edit.group)?;
        let path = edit.path.trim().trim_end_matches('/').to_string();
        if path != record.path {
            match self.workspace.state(&path) {
                PathState::OccupiedDirectory | PathState::EmptyDirectory => {}
                PathState::Missing => return Err(ProjectError::Missing(path)),
                PathState::NotDirectory => {
                    return Err(ProjectError::Invalid(format!(
                        "{path}은(는) 디렉토리가 아닙니다."
                    )));
                }
            }
        }
        for entry in self.store.list() {
            let Ok(other) = entry else { continue };
            if other.name == name {
                continue;
            }
            if other.name == new_name {
                return Err(ProjectError::Taken(new_name));
            }
            if other.path == path {
                return Err(ProjectError::Taken(format!("{path} ({})", other.name)));
            }
        }

        record.group = group;
        record.path = path;
        if new_name == name {
            self.store.replace(&record)?;
            return Ok(record);
        }
        // 디렉토리를 먼저 옮기고 그 안의 기록을 새 이름으로 쓴다. 쓰기가 실패하면 되돌린다.
        self.files.rename_project(name, &new_name)?;
        record.name = new_name.clone();
        if let Err(e) = self.store.replace(&record) {
            let _ = self.files.rename_project(&new_name, name);
            return Err(e);
        }
        Ok(record)
    }

    /// 등록을 해제한다. 기록 전체(배포 스크립트 포함)를 보관소로 옮기고 그 자리를 돌려준다.
    pub fn unregister(&self, name: &str) -> Result<String, ProjectError> {
        self.store.load(name)?;
        self.files.archive_project(name)
    }

    /// 환경의 이름 · 서버 계정 · 배포 경로 · 브랜치를 바꾼다.
    ///
    /// 서버 계정이나 배포 경로가 바뀌면 그 계정으로 서버를 다시 읽고, 처음 연결할 때와 같이
    /// 다른 레포나 git 이 아닌 파일이 있으면 거부한다.
    pub fn update_environment(
        &self,
        name: &str,
        environment: &str,
        edit: &EnvironmentEdit,
        progress: &dyn ProgressSink,
    ) -> Result<EditedEnvironment, ProjectError> {
        let mut record = self.store.load(name)?;
        let current = find_environment(&record.environments, environment)?.clone();
        let new_name = check_environment(&edit.name)?;
        if new_name != current.name && record.environments.iter().any(|e| e.name == new_name) {
            return Err(ProjectError::Invalid(format!(
                "환경 {new_name}은(는) 이미 있습니다."
            )));
        }
        let path = edit.path.trim().trim_end_matches('/').to_string();
        if !path.starts_with('/') || path.len() < 2 {
            return Err(ProjectError::Invalid(
                "배포 경로는 /로 시작하는 절대 경로여야 합니다.".into(),
            ));
        }
        let branch = edit.branch.trim().to_string();
        if branch.is_empty() {
            return Err(ProjectError::Invalid("배포할 브랜치를 입력하세요.".into()));
        }

        let mut next = current.clone();
        next.name = new_name.clone();
        next.path = path.clone();
        next.branch = branch;

        let server_changed =
            edit.server != current.server || edit.login != current.login || path != current.path;
        let checkout = if server_changed {
            let repo = github_repo(self.workspace, &record.path)?;
            let seat = seat_by(self.seats, &edit.server, &edit.login)?;
            let checkout = self.probe.checkout(&seat, &path, progress)?;
            accept_checkout(&checkout, &repo, &path)?;
            next.server = seat.server;
            next.login = seat.login;
            next.instance = None;
            next.address = None;
            Some(checkout)
        } else {
            None
        };

        let slot = record
            .environments
            .iter_mut()
            .find(|e| e.name == current.name)
            .ok_or_else(|| ProjectError::Missing(format!("환경 {environment}")))?;
        *slot = next.clone();

        if new_name != current.name {
            self.files
                .rename_environment(name, &current.name, &new_name)?;
        }
        if let Err(e) = self.store.replace(&record) {
            if new_name != current.name {
                let _ = self
                    .files
                    .rename_environment(name, &new_name, &current.name);
            }
            return Err(e);
        }
        Ok(EditedEnvironment {
            environment: next,
            checkout,
        })
    }

    /// 환경을 뺀다. 그 환경의 기록과 배포 스크립트를 보관소로 옮기고 그 자리를 돌려준다.
    pub fn remove_environment(
        &self,
        name: &str,
        environment: &str,
    ) -> Result<String, ProjectError> {
        let mut record = self.store.load(name)?;
        let removed = find_environment(&record.environments, environment)?.clone();
        let text =
            toml::to_string_pretty(&removed).map_err(|e| ProjectError::Storage(e.to_string()))?;
        let archived = self.files.archive_environment(name, environment, &text)?;
        record.environments.retain(|e| e.name != environment);
        self.store.replace(&record)?;
        Ok(archived)
    }
}
