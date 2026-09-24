//! Git 연결 — 원격 레포와 이 레포 전용 SSH 키를 로컬 레포에 잇는다.
//!
//! 순서: 점검 → (git init) → 원격 → 키 → 로컬 설정 → 접속 확인.
//! 원격에 무언가를 만들기 전에 로컬에서 거절할 수 있는 것은 전부 거절한다. 원격을 만든
//! 뒤에 실패하면 거기서 멈추고, 다시 실행하면 이미 있는 원격과 키를 그대로 이어 쓴다.
//!
//! 이 절차는 키를 지우지 않는다. 프로젝트 화면에서는 만들고 잇기만 한다.

use crate::key::RepoRef;
use crate::port::ProgressSink;

use super::scan::GitState;
use super::{ProjectError, ProjectStore, Workspace};

/// 이 레포에 쓸 수 있는 키 하나. 값은 없고 자리만 있다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoKey {
    pub purpose: String,
    pub account: String,
    pub write: bool,
    /// GitHub 등록까지 끝났다.
    pub usable: bool,
    /// 개인 키 파일의 절대 경로. `core.sshCommand` 에 적는다.
    pub private_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Private,
    Public,
}

/// 원격을 어디서 가져오는가.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteChoice {
    /// 이미 설정된 `origin` 을 그대로 쓴다.
    Current,
    /// GitHub 에 이미 있는 레포. 주소나 `owner/repo`.
    Existing(String),
    /// GitHub 에 새로 만든다.
    Create { repo: RepoRef, visibility: Visibility },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyChoice {
    /// 시크릿 저장소에 있는 그 용도의 키.
    Stored { purpose: String },
    /// 새로 발급해 GitHub 에 쓰기 권한으로 등록한다.
    Issue { purpose: String },
}

#[derive(Debug, Clone)]
pub struct GitRequest {
    /// 원격을 만들거나 키를 등록할 GitHub 계정.
    pub account: String,
    pub remote: RemoteChoice,
    pub key: KeyChoice,
}

/// 연결하기 전에 보여 줄 것.
#[derive(Debug, Clone)]
pub struct GitPlan {
    pub state: GitState,
    /// `origin` 이 GitHub 레포면 그 레포.
    pub repo: Option<RepoRef>,
    /// 그 레포에 쓸 수 있는, 시크릿 저장소의 키들.
    pub keys: Vec<RepoKey>,
    /// 지금 `core.sshCommand` 가 가리키는 개인 키 파일.
    pub current_key: Option<String>,
    /// 값이 원격에 올라갈 수 있는 환경 변수 파일. 비어 있어야 연결할 수 있다.
    pub exposed: Vec<String>,
}

#[derive(Debug)]
pub struct Linked {
    pub repo: RepoRef,
    pub key: RepoKey,
    pub created_repository: bool,
    pub issued_key: bool,
    /// 설정은 끝났지만 접속 확인이 실패했다면 그 이유.
    pub unreachable: Option<String>,
}

/// 레포에 쓸 키를 찾고 만드는 곳.
pub trait RepoKeys: Send + Sync {
    fn keys_for(&self, repo: &RepoRef) -> Vec<RepoKey>;
    /// 키를 만들어 GitHub 에 쓰기 권한 배포 키로 등록한다.
    fn issue(
        &self,
        account: &str,
        repo: &RepoRef,
        purpose: &str,
        progress: &dyn ProgressSink,
    ) -> Result<RepoKey, ProjectError>;
}

/// GitHub 레포를 만드는 곳.
pub trait RemoteRepos: Send + Sync {
    /// 새 레포를 만들고 SSH 주소를 돌려준다.
    fn create(
        &self,
        account: &str,
        repo: &RepoRef,
        visibility: Visibility,
        progress: &dyn ProgressSink,
    ) -> Result<String, ProjectError>;
}

/// 로컬 레포의 git 설정.
pub trait LocalRepository: Send + Sync {
    fn set_origin(&self, path: &str, url: &str) -> Result<(), ProjectError>;
    /// 이 레포에서만 이 키로 접속하게 한다. `~/.ssh/config` 는 건드리지 않는다.
    fn use_key(&self, path: &str, private_key: &str) -> Result<(), ProjectError>;
    /// `origin` 에 닿는지 확인한다. 읽기만 한다.
    fn reach(&self, path: &str, progress: &dyn ProgressSink) -> Result<(), ProjectError>;
}

/// GitHub SSH 주소. 새로 잇는 원격은 언제나 이 모양이다 — 키로 접속하려면 SSH 여야 한다.
pub fn ssh_url(repo: &RepoRef) -> String {
    format!("git@github.com:{}.git", repo.slug())
}

/// 고른 키를 쓸지, 새로 발급할지.
enum KeyPlan {
    Use(RepoKey),
    Issue(String),
}

pub struct GitLink<'a> {
    store: &'a dyn ProjectStore,
    workspace: &'a dyn Workspace,
    local: &'a dyn LocalRepository,
    keys: &'a dyn RepoKeys,
    remotes: &'a dyn RemoteRepos,
}

impl<'a> GitLink<'a> {
    pub fn new(
        store: &'a dyn ProjectStore,
        workspace: &'a dyn Workspace,
        local: &'a dyn LocalRepository,
        keys: &'a dyn RepoKeys,
        remotes: &'a dyn RemoteRepos,
    ) -> GitLink<'a> {
        GitLink {
            store,
            workspace,
            local,
            keys,
            remotes,
        }
    }

    pub fn plan(&self, name: &str) -> Result<GitPlan, ProjectError> {
        let record = self.store.load(name)?;
        let scan = self.workspace.scan(&record.path)?;
        let repo = match &scan.git {
            GitState::Remote { origin, .. } => RepoRef::parse(origin),
            GitState::Local { .. } | GitState::Absent => None,
        };
        let keys = repo.as_ref().map(|r| self.keys.keys_for(r)).unwrap_or_default();
        let exposed = scan
            .env_view()
            .into_iter()
            .filter(|f| f.exposed())
            .map(|f| f.name)
            .collect();
        Ok(GitPlan {
            state: scan.git,
            repo,
            keys,
            current_key: scan.ssh_key,
            exposed,
        })
    }

    pub fn connect(
        &self,
        name: &str,
        request: &GitRequest,
        progress: &dyn ProgressSink,
    ) -> Result<Linked, ProjectError> {
        let record = self.store.load(name)?;
        let plan = self.plan(name)?;
        if !plan.exposed.is_empty() {
            return Err(ProjectError::Invalid(format!(
                "{}의 값이 원격 레포에 올라갈 수 있습니다. .gitignore에 추가한 뒤 연결하세요.",
                plan.exposed.join(", ")
            )));
        }
        let origin = match &plan.state {
            GitState::Remote { origin, .. } => Some(origin.clone()),
            GitState::Local { .. } | GitState::Absent => None,
        };
        let target = self.target(origin.as_deref(), &request.remote)?;
        let known = self.keys.keys_for(&target);
        let key = self.pick_key(&known, &request.key)?;

        if plan.state == GitState::Absent {
            self.workspace.init_git(&record.path)?;
        }

        let mut created_repository = false;
        if origin.is_none() {
            let url = match &request.remote {
                RemoteChoice::Create { repo, visibility } => {
                    created_repository = true;
                    self.remotes.create(&request.account, repo, *visibility, progress)?
                }
                RemoteChoice::Existing(_) | RemoteChoice::Current => ssh_url(&target),
            };
            self.local.set_origin(&record.path, &url)?;
        }

        let (key, issued_key) = match key {
            KeyPlan::Use(stored) => (stored, false),
            KeyPlan::Issue(purpose) => (self.keys.issue(&request.account, &target, &purpose, progress)?, true),
        };
        self.local.use_key(&record.path, &key.private_key)?;

        let unreachable = self.local.reach(&record.path, progress).err().map(|e| e.to_string());
        Ok(Linked {
            repo: target,
            key,
            created_repository,
            issued_key,
            unreachable,
        })
    }

    /// 어느 레포에 이을 것인가. 이미 `origin` 이 있으면 그것만 쓴다 — 다른 레포로 바꾸는
    /// 일은 이 절차가 하지 않는다.
    fn target(&self, origin: Option<&str>, choice: &RemoteChoice) -> Result<RepoRef, ProjectError> {
        match (origin, choice) {
            (Some(url), RemoteChoice::Current) => RepoRef::parse(url).ok_or_else(|| {
                ProjectError::Invalid(format!("{url}은(는) GitHub 레포가 아닙니다."))
            }),
            (Some(url), _) => Err(ProjectError::Invalid(format!(
                "이미 origin이 {url}(으)로 설정되어 있습니다. 기존 원격을 사용하세요."
            ))),
            (None, RemoteChoice::Current) => {
                Err(ProjectError::Invalid("origin이 없습니다. 레포를 고르거나 새로 만드세요.".into()))
            }
            (None, RemoteChoice::Existing(text)) => RepoRef::parse(text).ok_or_else(|| {
                ProjectError::Invalid(format!("{text}을(를) GitHub 레포로 읽지 못했습니다."))
            }),
            (None, RemoteChoice::Create { repo, .. }) => Ok(repo.clone()),
        }
    }

    /// 같은 용도의 키가 이미 있으면 발급을 막는다 — 한 용도에 키는 하나다.
    fn pick_key(&self, known: &[RepoKey], choice: &KeyChoice) -> Result<KeyPlan, ProjectError> {
        match choice {
            KeyChoice::Stored { purpose } => {
                let key = known
                    .iter()
                    .find(|k| &k.purpose == purpose)
                    .ok_or_else(|| ProjectError::Missing(format!("{purpose} 키")))?;
                if !key.usable {
                    return Err(ProjectError::Invalid(format!(
                        "{purpose} 키는 GitHub 등록이 끝나지 않았습니다. 자격 증명 화면에서 등록을 다시 시도하세요."
                    )));
                }
                Ok(KeyPlan::Use(key.clone()))
            }
            KeyChoice::Issue { purpose } => {
                if known.iter().any(|k| &k.purpose == purpose) {
                    return Err(ProjectError::Invalid(format!(
                        "이 레포에 {purpose} 키가 이미 있습니다. 저장된 키를 선택하세요."
                    )));
                }
                Ok(KeyPlan::Issue(purpose.clone()))
            }
        }
    }
}
