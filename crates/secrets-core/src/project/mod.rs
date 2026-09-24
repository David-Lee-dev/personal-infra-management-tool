//! 프로젝트 — 로컬 디렉토리 하나에서 시작해 Git · 서버 · 자격 증명이 차례로 붙는 단위.
//!
//! 기록은 사람이 정한 것만 담는다(이름 · 그룹 · 경로 · 시작 방식). git 상태 · 런타임 ·
//! 환경 변수 파일처럼 디렉토리에서 읽을 수 있는 것은 기록하지 않고 볼 때마다 스캔한다.
//! 기록과 디렉토리가 어긋나는 일이 생기지 않게 하기 위해서다.
//!
//! 이 모듈은 디렉토리를 지우지 않는다. 만들기와 등록만 있다.
//!
//! ```text
//! projects/<이름>/project.toml
//! ```

pub mod env_file;
pub mod git_link;
pub mod naming;
pub mod runtime;
pub mod scan;
pub mod server_link;

use serde::{Deserialize, Serialize};

use crate::port::Clock;

pub use env_file::{EnvFileRole, EnvFileView};
pub use git_link::{
    GitLink, GitPlan, GitRequest, KeyChoice, Linked, LocalRepository, RemoteChoice, RemoteRepos,
    RepoKey, RepoKeys, Visibility,
};
pub use runtime::{DetectedRuntime, Runtime, RuntimeEvidence, RuntimeVerdict};
pub use scan::{EnvFileFact, GitState, LocalScan};
pub use server_link::{
    Attached, Checkout, Environment, ServerLink, ServerProbe, ServerRequest, ServerSeat, ServerSeats,
};

/// 프로젝트를 어떻게 시작했는가.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// 이 도구가 빈 디렉토리를 만들었다.
    Created,
    /// 이미 있던 디렉토리를 등록했다.
    Registered,
}

/// `project.toml` 의 모양.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRecord {
    pub name: String,
    pub group: String,
    /// 절대 경로.
    pub path: String,
    pub origin: Origin,
    pub created_at: String,
    /// 붙은 서버 환경. 서버 연결에서 더해진다.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<Environment>,
}

/// 경로에 지금 무엇이 있는가.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathState {
    Missing,
    EmptyDirectory,
    OccupiedDirectory,
    /// 디렉토리가 아닌 무언가가 있다.
    NotDirectory,
}

/// 새 디렉토리에서 git 을 어떻게 시작할까.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitStart {
    /// `git init` 까지 한다. 원격은 붙이지 않는다.
    Init,
    /// 디렉토리만 만든다.
    None,
}

#[derive(Debug, Clone)]
pub struct NewProject {
    pub name: String,
    pub group: String,
    /// 상위 디렉토리. 절대 경로.
    pub parent: String,
    pub directory: String,
    pub git: GitStart,
}

#[derive(Debug, Clone)]
pub struct Registration {
    pub name: String,
    pub group: String,
    /// 이미 있는 디렉토리. 절대 경로.
    pub path: String,
}

#[derive(Debug)]
pub enum ProjectError {
    /// 사람이 적은 값이 쓸 수 없는 모양이다.
    Invalid(String),
    Missing(String),
    /// 같은 이름이나 같은 경로가 이미 프로젝트다.
    Taken(String),
    Storage(String),
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectError::Invalid(detail) => write!(f, "{detail}"),
            ProjectError::Missing(what) => write!(f, "{what}을(를) 찾을 수 없습니다."),
            ProjectError::Taken(what) => write!(f, "{what}은(는) 이미 프로젝트로 등록되어 있습니다."),
            ProjectError::Storage(detail) => write!(f, "{detail}"),
        }
    }
}

/// 프로젝트 기록이 놓이는 곳.
pub trait ProjectStore: Send + Sync {
    /// 전부. 읽지 못한 기록은 건너뛰지 않고 오류로 돌려준다.
    fn list(&self) -> Vec<Result<ProjectRecord, String>>;
    fn load(&self, name: &str) -> Result<ProjectRecord, ProjectError>;
    /// 새 기록을 쓴다. 같은 이름의 기록이 있으면 `Taken` 이다.
    fn insert(&self, record: &ProjectRecord) -> Result<(), ProjectError>;
    /// 있는 기록을 통째로 다시 쓴다. 기록이 없으면 `Missing` 이다.
    fn replace(&self, record: &ProjectRecord) -> Result<(), ProjectError>;
}

/// 프로젝트 디렉토리가 놓이는 로컬 작업 공간.
pub trait Workspace: Send + Sync {
    /// `parent` 아래 `directory` 의 절대 경로. 만들지는 않는다.
    fn join(&self, parent: &str, directory: &str) -> String;
    fn state(&self, path: &str) -> PathState;
    /// 디렉토리를 만든다. 이미 있으면 빈 디렉토리일 때만 그대로 쓴다.
    fn create_directory(&self, path: &str) -> Result<(), ProjectError>;
    /// 원격 없이 로컬 저장소만 만든다.
    fn init_git(&self, path: &str) -> Result<(), ProjectError>;
    /// 디렉토리를 읽는다. 값은 읽지 않는다 — 이름 · 개수 · 근거 파일만.
    fn scan(&self, path: &str) -> Result<LocalScan, ProjectError>;
    /// 뿌리의 `.gitignore` 끝에 이름들을 더한다. 이미 있는 줄은 다시 쓰지 않는다.
    fn add_to_gitignore(&self, path: &str, names: &[String]) -> Result<(), ProjectError>;
}

/// 단계 하나의 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StageState {
    Done,
    Warn,
    Pending,
}

/// 로컬 · Git · 서버. 자격 증명은 단계가 아니라 필요할 때 붙는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stages {
    pub local: StageState,
    pub git: StageState,
    pub server: StageState,
}

impl Stages {
    /// 스캔과 붙은 환경으로 단계를 판정한다.
    pub fn of(scan: &Result<LocalScan, ProjectError>, environments: &[Environment]) -> Stages {
        let server = if environments.is_empty() {
            StageState::Pending
        } else {
            StageState::Done
        };
        let Ok(scan) = scan else {
            return Stages {
                local: StageState::Warn,
                git: StageState::Pending,
                server,
            };
        };
        let local = if scan.env_view().iter().any(|f| f.exposed()) {
            StageState::Warn
        } else {
            StageState::Done
        };
        let git = match &scan.git {
            GitState::Remote { .. } => StageState::Done,
            GitState::Local { .. } | GitState::Absent => StageState::Pending,
        };
        Stages { local, git, server }
    }
}

/// 기록과 그 디렉토리를 지금 읽은 결과.
#[derive(Debug)]
pub struct Overview {
    pub record: ProjectRecord,
    pub scan: Result<LocalScan, ProjectError>,
    pub stages: Stages,
}

/// 만들기의 결과. 디렉토리와 기록은 생겼지만 뒤따른 단계가 실패했을 수 있다.
#[derive(Debug)]
pub struct Created {
    pub record: ProjectRecord,
    /// 실패한 뒤따른 단계. 비어 있으면 전부 됐다.
    pub incomplete: Vec<String>,
}

/// 프로젝트를 만들고 등록하고 읽는 일.
pub struct Projects<'a> {
    store: &'a dyn ProjectStore,
    workspace: &'a dyn Workspace,
    clock: &'a dyn Clock,
}

impl<'a> Projects<'a> {
    pub fn new(
        store: &'a dyn ProjectStore,
        workspace: &'a dyn Workspace,
        clock: &'a dyn Clock,
    ) -> Projects<'a> {
        Projects {
            store,
            workspace,
            clock,
        }
    }

    /// 빈 디렉토리를 만들고 프로젝트로 기록한다.
    ///
    /// 순서: 검사 → 디렉토리 → 기록 → git. 기록은 디렉토리가 생긴 뒤에만 쓴다 —
    /// 기록이 보이면 디렉토리가 있다는 뜻이어야 한다. git 은 기록 뒤에 둔다. 실패해도
    /// 디렉토리는 이미 프로젝트이고, git 은 나중에 다시 시작할 수 있기 때문이다.
    pub fn create(&self, request: &NewProject) -> Result<Created, ProjectError> {
        let name = naming::check_name(&request.name)?;
        let directory = naming::check_directory(&request.directory)?;
        let group = naming::check_group(&request.group)?;

        match self.workspace.state(&request.parent) {
            PathState::OccupiedDirectory | PathState::EmptyDirectory => {}
            PathState::Missing => return Err(ProjectError::Missing(request.parent.clone())),
            PathState::NotDirectory => {
                return Err(ProjectError::Invalid(format!(
                    "{}은(는) 디렉토리가 아닙니다.",
                    request.parent
                )));
            }
        }
        let path = self.workspace.join(&request.parent, &directory);
        match self.workspace.state(&path) {
            PathState::Missing | PathState::EmptyDirectory => {}
            PathState::OccupiedDirectory => {
                return Err(ProjectError::Invalid(format!(
                    "{path}에 이미 파일이 있습니다. 이 디렉토리를 쓰려면 '기존 디렉토리 등록'을 선택하세요."
                )));
            }
            PathState::NotDirectory => {
                return Err(ProjectError::Invalid(format!("{path}에 디렉토리가 아닌 파일이 있습니다.")));
            }
        }
        self.ensure_free(&name, &path)?;

        self.workspace.create_directory(&path)?;
        let record = self.record(name, group, path, Origin::Created);
        self.store.insert(&record)?;

        let mut incomplete = Vec::new();
        if request.git == GitStart::Init
            && let Err(e) = self.workspace.init_git(&record.path)
        {
            incomplete.push(format!("git init을 하지 못했습니다: {e}"));
        }
        Ok(Created { record, incomplete })
    }

    /// 이미 있는 디렉토리를 프로젝트로 기록한다. 디렉토리는 바꾸지 않는다.
    pub fn register(&self, request: &Registration) -> Result<ProjectRecord, ProjectError> {
        let name = naming::check_name(&request.name)?;
        let group = naming::check_group(&request.group)?;
        match self.workspace.state(&request.path) {
            PathState::OccupiedDirectory | PathState::EmptyDirectory => {}
            PathState::Missing => return Err(ProjectError::Missing(request.path.clone())),
            PathState::NotDirectory => {
                return Err(ProjectError::Invalid(format!(
                    "{}은(는) 디렉토리가 아닙니다.",
                    request.path
                )));
            }
        }
        self.ensure_free(&name, &request.path)?;

        let record = self.record(name, group, request.path.clone(), Origin::Registered);
        self.store.insert(&record)?;
        Ok(record)
    }

    /// 경로 하나를 미리 본다. 등록 전 확인과 새 디렉토리 경로 검사에 쓴다.
    pub fn inspect(&self, path: &str) -> (PathState, Option<LocalScan>) {
        let state = self.workspace.state(path);
        let scan = match state {
            PathState::OccupiedDirectory | PathState::EmptyDirectory => self.workspace.scan(path).ok(),
            PathState::Missing | PathState::NotDirectory => None,
        };
        (state, scan)
    }

    /// 값이 원격에 올라갈 수 있는 환경 변수 파일을 `.gitignore` 에 더한다. 더한 이름을 돌려준다.
    pub fn ignore_exposed(&self, name: &str) -> Result<Vec<String>, ProjectError> {
        let record = self.store.load(name)?;
        let scan = self.workspace.scan(&record.path)?;
        let exposed: Vec<String> = scan
            .env_view()
            .into_iter()
            .filter(|f| f.exposed() && !f.tracked)
            .map(|f| f.name)
            .collect();
        if !exposed.is_empty() {
            self.workspace.add_to_gitignore(&record.path, &exposed)?;
        }
        Ok(exposed)
    }

    pub fn overview(&self, name: &str) -> Result<Overview, ProjectError> {
        let record = self.store.load(name)?;
        Ok(self.read(record))
    }

    /// 전부 읽는다. 기록을 읽지 못한 것은 오류로 따로 돌려준다.
    pub fn list(&self) -> (Vec<Overview>, Vec<String>) {
        let mut found = Vec::new();
        let mut errors = Vec::new();
        for entry in self.store.list() {
            match entry {
                Ok(record) => found.push(self.read(record)),
                Err(message) => errors.push(message),
            }
        }
        found.sort_by(|a, b| {
            (a.record.group.as_str(), a.record.name.as_str())
                .cmp(&(b.record.group.as_str(), b.record.name.as_str()))
        });
        (found, errors)
    }

    fn read(&self, record: ProjectRecord) -> Overview {
        let scan = match self.workspace.state(&record.path) {
            PathState::OccupiedDirectory | PathState::EmptyDirectory => self.workspace.scan(&record.path),
            PathState::Missing | PathState::NotDirectory => {
                Err(ProjectError::Missing(record.path.clone()))
            }
        };
        let stages = Stages::of(&scan, &record.environments);
        Overview {
            record,
            scan,
            stages,
        }
    }

    fn ensure_free(&self, name: &str, path: &str) -> Result<(), ProjectError> {
        for entry in self.store.list() {
            let Ok(existing) = entry else { continue };
            if existing.name == name {
                return Err(ProjectError::Taken(name.to_string()));
            }
            if existing.path == path {
                return Err(ProjectError::Taken(format!("{path} ({})", existing.name)));
            }
        }
        Ok(())
    }

    fn record(&self, name: String, group: String, path: String, origin: Origin) -> ProjectRecord {
        ProjectRecord {
            name,
            group,
            path,
            origin,
            created_at: self.clock.now(),
            environments: Vec::new(),
        }
    }
}
