//! 배포 — 환경마다 사용자가 쓴 배포 스크립트로 서버에 배포한다. 한 환경에 이름 붙인 스크립트를
//! 여러 개 둘 수 있고(`deploy` · `migrate` · `restart` …), 배포할 때 하나를 고른다.
//!
//! 서버마다 런타임과 도구(systemd · pm2 …)가 다르므로 이 도구는 배포 방법을 정하지 않는다.
//! 스크립트는 사용자가 쓰고, 이 도구는 스크립트를 보관하고 고칠 수 있게 하며, 배포는 반드시
//! 그 스크립트를 통한다. 스크립트는 자격 증명이 아니다 — 값은 서버의 환경 변수 파일에 있다.
//!
//! ```text
//! projects/<프로젝트>/deploy/<환경>/<스크립트>.sh
//! ```
//!
//! 배포 전에 로컬 · 원격 · 서버의 커밋을 견주어 보여 준다. 코드가 서로 달라도 **막지 않는다** —
//! 알리기만 한다. 막는 것은 둘뿐이다: 고른 배포 스크립트가 없을 때, 환경 변수 파일이 로컬과
//! 서버에서 다를 때.

use crate::port::{Channel, ProgressSink};

use super::env_sync::{EnvComparison, EnvSync};
use super::scan::GitState;
use super::server_link::{
    Checkout, ServerProbe, ServerSeat, ServerSeats, find_environment, seat_of,
};
use super::{ProjectError, ProjectStore, Workspace};

/// 스크립트가 실행될 때 이 도구가 넘겨 주는 환경 변수. 이름과 뜻.
pub const SCRIPT_VARIABLES: &[(&str, &str)] = &[
    ("DEPLOY_ENV", "환경 이름 (예: prod)"),
    ("DEPLOY_PATH", "서버의 배포 경로"),
    ("DEPLOY_BRANCH", "이 환경이 배포하는 브랜치"),
    ("DEPLOY_ENV_FILE", "서버의 환경 변수 파일 경로"),
];

/// 배포 스크립트가 놓이는 곳. 스크립트 이름은 [`check_script_name`] 을 거친 것이다.
pub trait DeployScripts: Send + Sync {
    /// 이 환경의 스크립트 이름들. 이름 순.
    fn names(&self, project: &str, environment: &str) -> Result<Vec<String>, ProjectError>;
    /// 스크립트 파일의 자리. 없어도 알려 준다.
    fn location(&self, project: &str, environment: &str, script: &str) -> String;
    fn load(
        &self,
        project: &str,
        environment: &str,
        script: &str,
    ) -> Result<Option<String>, ProjectError>;
    /// 스크립트를 쓴다. 있던 스크립트는 보관소로 옮기고 그 자리를 돌려준다.
    fn save(
        &self,
        project: &str,
        environment: &str,
        script: &str,
        text: &str,
    ) -> Result<Option<String>, ProjectError>;
    /// 스크립트를 보관소로 옮긴다. 옮긴 자리를 돌려주고, 없었으면 `None`.
    fn remove(
        &self,
        project: &str,
        environment: &str,
        script: &str,
    ) -> Result<Option<String>, ProjectError>;
}

/// 스크립트 이름. 파일 이름(`<이름>.sh`)이 되므로 영문 · 숫자 · `-` · `_` 만 받는다.
pub fn check_script_name(text: &str) -> Result<String, ProjectError> {
    let name = text.trim();
    let valid = !name.is_empty()
        && name.len() <= 40
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    if valid {
        Ok(name.to_string())
    } else {
        Err(ProjectError::Invalid(
            "스크립트 이름에는 영문 · 숫자 · - · _만 쓸 수 있고, -로 시작할 수 없습니다.".into(),
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployScript {
    pub name: String,
    pub path: String,
    /// 아직 쓰지 않았으면 없다.
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedScript {
    pub path: String,
    /// 내용이 같아 쓰지 않았다.
    pub unchanged: bool,
    /// 이전 스크립트를 옮긴 자리.
    pub archived: Option<String>,
}

/// 줄 끝을 `\n` 으로 맞추고 마지막 줄을 끝맺는다. 내용은 바꾸지 않는다.
pub fn normalize(text: &str) -> Result<String, ProjectError> {
    if text.contains('\0') {
        return Err(ProjectError::Invalid(
            "스크립트에 NUL 문자가 있습니다.".into(),
        ));
    }
    let mut body = text.replace("\r\n", "\n").replace('\r', "\n");
    if body.trim().is_empty() {
        return Err(ProjectError::Invalid(
            "배포 스크립트가 비어 있습니다.".into(),
        ));
    }
    if !body.ends_with('\n') {
        body.push('\n');
    }
    Ok(body)
}

pub struct Deployment<'a> {
    store: &'a dyn ProjectStore,
    scripts: &'a dyn DeployScripts,
}

impl<'a> Deployment<'a> {
    pub fn new(store: &'a dyn ProjectStore, scripts: &'a dyn DeployScripts) -> Deployment<'a> {
        Deployment { store, scripts }
    }

    /// 이 환경의 배포 스크립트 이름들. 읽기만 한다.
    pub fn scripts(&self, name: &str, environment: &str) -> Result<Vec<String>, ProjectError> {
        self.environment_of(name, environment)?;
        self.scripts.names(name, environment)
    }

    /// 이 환경의 배포 스크립트 하나. 읽기만 한다.
    pub fn script(
        &self,
        name: &str,
        environment: &str,
        script: &str,
    ) -> Result<DeployScript, ProjectError> {
        self.environment_of(name, environment)?;
        let script = check_script_name(script)?;
        Ok(DeployScript {
            path: self.scripts.location(name, environment, &script),
            text: self.scripts.load(name, environment, &script)?,
            name: script,
        })
    }

    /// 이 환경의 배포 스크립트를 쓴다. 없는 이름이면 새로 만든다. 내용이 같으면 쓰지 않는다.
    pub fn save_script(
        &self,
        name: &str,
        environment: &str,
        script: &str,
        text: &str,
    ) -> Result<SavedScript, ProjectError> {
        self.environment_of(name, environment)?;
        let script = check_script_name(script)?;
        let body = normalize(text)?;
        let path = self.scripts.location(name, environment, &script);
        if self.scripts.load(name, environment, &script)?.as_deref() == Some(body.as_str()) {
            return Ok(SavedScript {
                path,
                unchanged: true,
                archived: None,
            });
        }
        let archived = self.scripts.save(name, environment, &script, &body)?;
        Ok(SavedScript {
            path,
            unchanged: false,
            archived,
        })
    }

    /// 이 환경의 배포 스크립트를 보관소로 옮긴다. 옮긴 자리를 돌려준다.
    pub fn remove_script(
        &self,
        name: &str,
        environment: &str,
        script: &str,
    ) -> Result<String, ProjectError> {
        self.environment_of(name, environment)?;
        let script = check_script_name(script)?;
        self.scripts
            .remove(name, environment, &script)?
            .ok_or_else(|| ProjectError::Missing(format!("배포 스크립트 {script}")))
    }

    fn environment_of(&self, name: &str, environment: &str) -> Result<(), ProjectError> {
        let record = self.store.load(name)?;
        find_environment(&record.environments, environment).map(|_| ())
    }
}

/* ── 배포 ─────────────────────────────────────────────── */

/// 커밋 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revision {
    pub sha: String,
    pub subject: String,
}

impl Revision {
    /// 줄인 sha 끼리도 맞춰 본다.
    pub fn same_as(&self, other: &Revision) -> bool {
        self.sha.starts_with(&other.sha) || other.sha.starts_with(&self.sha)
    }
}

/// 로컬 레포에서 커밋을 읽는다. 작업 트리와 로컬 브랜치는 바꾸지 않는다.
pub trait LocalRevisions: Send + Sync {
    /// `origin` 의 그 브랜치를 가져온다 — 원격 추적 브랜치만 바뀐다.
    fn fetch(
        &self,
        path: &str,
        branch: &str,
        progress: &dyn ProgressSink,
    ) -> Result<(), ProjectError>;
    /// 브랜치 · 원격 브랜치 · 커밋(줄인 sha 도)을 커밋으로. 없으면 `None`.
    fn resolve(&self, path: &str, reference: &str) -> Option<Revision>;
    /// `to` 에는 있고 `from` 에는 없는 커밋 수(`from..to`). 셀 수 없으면 `None`.
    fn count(&self, path: &str, from: &str, to: &str) -> Option<u32>;
}

/// 서버 계정으로 들어가 배포 스크립트를 돌린다. 출력은 그대로 흐른다.
pub trait DeployRunner: Send + Sync {
    fn run(
        &self,
        seat: &ServerSeat,
        variables: &[(&str, String)],
        script: &str,
        progress: &dyn ProgressSink,
    ) -> Result<(), ProjectError>;
}

/// 코드에 대해 알릴 것. 무엇도 배포를 막지 않는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeNote {
    /// 원격을 가져오지 못했다. 마지막으로 받아 둔 정보로 견준다.
    FetchFailed(String),
    /// 원격에 그 브랜치가 없다. 스크립트가 받을 코드가 없다.
    RemoteMissing,
    /// 로컬에 그 브랜치가 없다.
    LocalMissing,
    /// 로컬이 지금 다른 브랜치에 있다.
    OtherBranch(String),
    /// 커밋하지 않은 변경. 배포되지 않는다.
    Uncommitted(u32),
    /// 로컬에만 있는 커밋 — 푸시하지 않았다. 배포되지 않는다.
    LocalAhead(u32),
    /// 원격에만 있는 커밋 — 로컬에서 받지 않은 커밋이 배포된다.
    LocalBehind(u32),
    /// 서버를 읽지 못했다.
    ServerUnreadable(String),
    /// 서버의 배포 경로가 git 저장소가 아니다.
    ServerNotRepository,
    /// 서버가 다른 브랜치에 있다.
    ServerOtherBranch(String),
    /// 서버에만 있는 커밋. 원격으로 fast-forward 하는 스크립트는 실패한다.
    ServerAhead(u32),
    /// 서버의 커밋을 로컬에서 찾지 못해 견줄 수 없다.
    ServerUnknownCommit(String),
    /// 환경 변수 파일을 고르지 않아 견주지 않았다.
    EnvNotChosen,
}

/// 배포를 막는 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocker {
    NoScript,
    /// 환경 변수 파일이 서버와 다르다(서버에 없음 · 배포 경로 없음 포함).
    EnvDiffers,
    /// 환경 변수 파일을 견주지 못했다.
    EnvUnchecked(String),
}

#[derive(Debug, Clone)]
pub struct DeployPlan {
    pub environment: String,
    /// 고른 스크립트 이름. 고르지 않았으면 없다.
    pub script_name: Option<String>,
    pub branch: String,
    /// 로컬의 그 브랜치.
    pub local: Option<Revision>,
    /// `origin/<브랜치>` — 스크립트가 받을 커밋.
    pub remote: Option<Revision>,
    /// 서버에 지금 있는 커밋.
    pub server: Option<Revision>,
    /// 배포하면 서버에 새로 들어가는 커밋 수.
    pub incoming: Option<u32>,
    pub notes: Vec<CodeNote>,
    /// 로컬 · 원격 · 서버가 같은 커밋이고 커밋 안 한 변경도 없다.
    pub same: bool,
    pub env: Option<EnvComparison>,
    pub script: Option<String>,
    pub blockers: Vec<Blocker>,
}

#[derive(Debug, Clone)]
pub struct Deployed {
    pub before: Option<Revision>,
    pub after: Option<Revision>,
}

pub struct Deployer<'a> {
    store: &'a dyn ProjectStore,
    workspace: &'a dyn Workspace,
    seats: &'a dyn ServerSeats,
    probe: &'a dyn ServerProbe,
    revisions: &'a dyn LocalRevisions,
    env: &'a EnvSync<'a>,
    scripts: &'a dyn DeployScripts,
    runner: &'a dyn DeployRunner,
}

impl<'a> Deployer<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        store: &'a dyn ProjectStore,
        workspace: &'a dyn Workspace,
        seats: &'a dyn ServerSeats,
        probe: &'a dyn ServerProbe,
        revisions: &'a dyn LocalRevisions,
        env: &'a EnvSync<'a>,
        scripts: &'a dyn DeployScripts,
        runner: &'a dyn DeployRunner,
    ) -> Deployer<'a> {
        Deployer {
            store,
            workspace,
            seats,
            probe,
            revisions,
            env,
            scripts,
            runner,
        }
    }

    /// 배포 전에 볼 것 — 로컬 · 원격 · 서버의 커밋, 알릴 것, 막는 것. 로컬 레포의 원격 추적
    /// 브랜치를 갱신하는 것 말고는 아무것도 바꾸지 않는다. 스크립트를 고르지 않았으면
    /// 스크립트가 없는 것으로 막는다.
    pub fn plan(
        &self,
        name: &str,
        environment: &str,
        script: Option<&str>,
        progress: &dyn ProgressSink,
    ) -> Result<DeployPlan, ProjectError> {
        let script_name = script.map(check_script_name).transpose()?;
        let record = self.store.load(name)?;
        let env = find_environment(&record.environments, environment)?.clone();
        let path = record.path.as_str();
        let branch = env.branch.clone();
        let mut notes = Vec::new();

        if let Err(e) = self.revisions.fetch(path, &branch, progress) {
            notes.push(CodeNote::FetchFailed(e.to_string()));
        }
        let local_ref = format!("refs/heads/{branch}");
        let remote_ref = format!("refs/remotes/origin/{branch}");
        let local = self.revisions.resolve(path, &local_ref);
        let remote = self.revisions.resolve(path, &remote_ref);

        let mut dirty = false;
        if let Ok(scan) = self.workspace.scan(path)
            && let GitState::Remote {
                branch: current,
                changes,
                ..
            }
            | GitState::Local {
                branch: current,
                changes,
                ..
            } = &scan.git
        {
            if let Some(current) = current
                && *current != branch
            {
                notes.push(CodeNote::OtherBranch(current.clone()));
                dirty = true;
            }
            if *changes > 0 {
                notes.push(CodeNote::Uncommitted(*changes));
                dirty = true;
            }
        }

        match (&local, &remote) {
            (_, None) => notes.push(CodeNote::RemoteMissing),
            (None, Some(_)) => notes.push(CodeNote::LocalMissing),
            (Some(_), Some(_)) => {
                if let Some(n) = self
                    .revisions
                    .count(path, &remote_ref, &local_ref)
                    .filter(|n| *n > 0)
                {
                    notes.push(CodeNote::LocalAhead(n));
                }
                if let Some(n) = self
                    .revisions
                    .count(path, &local_ref, &remote_ref)
                    .filter(|n| *n > 0)
                {
                    notes.push(CodeNote::LocalBehind(n));
                }
            }
        }

        let seat = seat_of(self.seats, &env)?;
        let mut server = None;
        let mut incoming = None;
        match self.probe.checkout(&seat, &env.path, progress) {
            Err(e) => notes.push(CodeNote::ServerUnreadable(e.to_string())),
            Ok(Checkout::Repository {
                branch: server_branch,
                commit,
                ..
            }) => {
                if let Some(b) = server_branch.filter(|b| *b != branch) {
                    notes.push(CodeNote::ServerOtherBranch(b));
                }
                if let Some(short) = commit {
                    match self.revisions.resolve(path, &short) {
                        None => notes.push(CodeNote::ServerUnknownCommit(short)),
                        Some(found) => {
                            if remote.is_some() {
                                incoming = self.revisions.count(path, &found.sha, &remote_ref);
                                if let Some(n) = self
                                    .revisions
                                    .count(path, &remote_ref, &found.sha)
                                    .filter(|n| *n > 0)
                                {
                                    notes.push(CodeNote::ServerAhead(n));
                                }
                            }
                            server = Some(found);
                        }
                    }
                }
            }
            Ok(_) => notes.push(CodeNote::ServerNotRepository),
        }

        let mut blockers = Vec::new();
        let script = match &script_name {
            Some(s) => self
                .scripts
                .load(name, environment, s)?
                .map(|_| self.scripts.location(name, environment, s)),
            None => None,
        };
        if script.is_none() {
            blockers.push(Blocker::NoScript);
        }
        let env_check = if env.env_file.is_none() {
            notes.push(CodeNote::EnvNotChosen);
            None
        } else {
            match self.env.compare(name, environment, progress) {
                Ok(found) => {
                    if !found.state.same() {
                        blockers.push(Blocker::EnvDiffers);
                    }
                    Some(found)
                }
                Err(e) => {
                    blockers.push(Blocker::EnvUnchecked(e.to_string()));
                    None
                }
            }
        };

        let same = !dirty
            && matches!((&local, &remote, &server), (Some(l), Some(r), Some(s)) if l.same_as(r) && r.same_as(s));
        Ok(DeployPlan {
            environment: env.name,
            script_name,
            branch,
            local,
            remote,
            server,
            incoming,
            notes,
            same,
            env: env_check,
            script,
            blockers,
        })
    }

    /// 고른 배포 스크립트를 서버에서 돌린다. 막는 것을 바로 앞에서 한 번 더 확인한다.
    pub fn run(
        &self,
        name: &str,
        environment: &str,
        script: &str,
        progress: &dyn ProgressSink,
    ) -> Result<Deployed, ProjectError> {
        let script_name = check_script_name(script)?;
        let record = self.store.load(name)?;
        let env = find_environment(&record.environments, environment)?.clone();
        let script = self
            .scripts
            .load(name, environment, &script_name)?
            .ok_or_else(|| {
                ProjectError::Invalid(format!(
                    "환경 {environment}에 배포 스크립트 {script_name}이(가) 없습니다."
                ))
            })?;
        if env.env_file.is_some() {
            progress.line(Channel::Step, "환경 변수 비교");
            let found = self.env.compare(name, environment, progress)?;
            if !found.state.same() {
                return Err(ProjectError::Invalid(format!(
                    "{}이(가) 로컬 {}과(와) 다릅니다. 환경 변수를 먼저 반영하세요.",
                    found.server_file, found.local_file
                )));
            }
        }
        let seat = seat_of(self.seats, &env)?;
        progress.line(Channel::Step, "서버의 지금 커밋 읽기");
        let before = self.server_revision(&record.path, &seat, &env.path, progress);

        let variables = [
            ("DEPLOY_ENV", env.name.clone()),
            ("DEPLOY_PATH", env.path.clone()),
            ("DEPLOY_BRANCH", env.branch.clone()),
            (
                "DEPLOY_ENV_FILE",
                format!("{}/{}", env.path.trim_end_matches('/'), env.server_env_file),
            ),
        ];
        progress.line(Channel::Step, "배포 스크립트 실행");
        self.runner
            .run(&seat, &variables, &script, progress)
            .map_err(|e| ProjectError::Storage(format!("배포 스크립트가 실패했습니다: {e}")))?;

        progress.line(Channel::Step, "배포 결과 읽기");
        let after = self.server_revision(&record.path, &seat, &env.path, progress);
        // 마지막 줄은 서버를 다시 읽은 출력이라, 끝났다는 것을 따로 적는다.
        let short = |r: &Option<Revision>| {
            r.as_ref()
                .map_or("—".to_string(), |r| r.sha.chars().take(7).collect())
        };
        progress.line(
            Channel::Out,
            &format!("배포 끝 · 서버 {} → {}", short(&before), short(&after)),
        );
        Ok(Deployed { before, after })
    }

    fn server_revision(
        &self,
        path: &str,
        seat: &ServerSeat,
        dir: &str,
        progress: &dyn ProgressSink,
    ) -> Option<Revision> {
        match self.probe.checkout(seat, dir, progress).ok()? {
            Checkout::Repository {
                commit: Some(short),
                ..
            } => Some(self.revisions.resolve(path, &short).unwrap_or(Revision {
                sha: short,
                subject: String::new(),
            })),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod check_script_name {
        use super::*;

        #[test]
        fn keeps_a_plain_name_trimmed() {
            assert_eq!(check_script_name(" db_migrate-2 ").unwrap(), "db_migrate-2");
        }

        #[test]
        fn rejects_path_pieces_dots_and_leading_dashes() {
            for bad in ["", "a/b", "..", "a.b", "-x", "배포", "a b"] {
                assert!(check_script_name(bad).is_err(), "{bad}");
            }
        }
    }

    mod normalize {
        use super::*;

        #[test]
        fn ends_every_line_with_a_newline() {
            assert_eq!(normalize("a\r\nb").unwrap(), "a\nb\n");
            assert_eq!(normalize("a\n").unwrap(), "a\n");
        }

        #[test]
        fn refuses_blank_and_binary_text() {
            assert!(normalize("  \n\t").is_err());
            assert!(normalize("a\0b").is_err());
        }
    }
}
