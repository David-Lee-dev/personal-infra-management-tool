//! 배포 — 코드가 달라도 막지 않고 알리기만 하는가, 막는 것은 스크립트와 환경 변수뿐인가.

use std::collections::BTreeMap;
use std::sync::Mutex;

use secrets_core::port::{ProgressSink, Silent};
use secrets_core::project::{
    Blocker, Checkout, CodeNote, DeployRunner, DeployScripts, Deployer, EnvDigest, EnvSync,
    Environment, GitState, LocalEnvFiles, LocalRevisions, LocalScan, Origin, PathState,
    ProjectError, ProjectRecord, ProjectStore, RepoTracking, Revision, ServerEnv, ServerEnvFiles,
    ServerProbe, ServerSeat, ServerSeats, Workspace,
};

struct Store {
    env_file: Option<&'static str>,
}

impl ProjectStore for Store {
    fn list(&self) -> Vec<Result<ProjectRecord, String>> {
        vec![Ok(self.load("api").unwrap())]
    }
    fn load(&self, _: &str) -> Result<ProjectRecord, ProjectError> {
        Ok(ProjectRecord {
            name: "api".into(),
            group: "g".into(),
            path: "/w/api".into(),
            origin: Origin::Registered,
            created_at: "t".into(),
            environments: vec![Environment {
                name: "prod".into(),
                server: "i-1".into(),
                instance: None,
                address: None,
                login: "app".into(),
                path: "/srv/api".into(),
                branch: "main".into(),
                connected_at: "t".into(),
                env_file: self.env_file.map(str::to_string),
                server_env_file: ".env.local".into(),
            }],
        })
    }
    fn insert(&self, _: &ProjectRecord) -> Result<(), ProjectError> {
        unreachable!()
    }
    fn replace(&self, _: &ProjectRecord) -> Result<(), ProjectError> {
        unreachable!("배포는 기록을 고치지 않는다")
    }
}

struct Disk {
    branch: &'static str,
    changes: u32,
}

impl Workspace for Disk {
    fn join(&self, a: &str, b: &str) -> String {
        format!("{a}/{b}")
    }
    fn state(&self, _: &str) -> PathState {
        PathState::OccupiedDirectory
    }
    fn create_directory(&self, _: &str) -> Result<(), ProjectError> {
        unreachable!()
    }
    fn init_git(&self, _: &str) -> Result<(), ProjectError> {
        unreachable!()
    }
    fn scan(&self, _: &str) -> Result<LocalScan, ProjectError> {
        Ok(LocalScan {
            git: GitState::Remote {
                branch: Some(self.branch.into()),
                commits: 3,
                changes: self.changes,
                origin: "git@github.com:O/api.git".into(),
            },
            runtimes: vec![],
            env_files: vec![],
            ssh_key: None,
        })
    }
    fn add_to_gitignore(&self, _: &str, _: &[String]) -> Result<(), ProjectError> {
        unreachable!()
    }
}

struct Seats;

impl ServerSeats for Seats {
    fn seats(&self) -> Vec<ServerSeat> {
        vec![ServerSeat {
            server: "i-1".into(),
            server_name: "web".into(),
            kind: "ec2".into(),
            address: "3.3.3.3".into(),
            port: 22,
            login: "app".into(),
            admin: false,
            verified: true,
            key: Some("/k".into()),
        }]
    }
}

/// 서버가 차례로 답하는 커밋(줄인 sha). 비면 마지막 것을 되풀이한다.
struct Probe(Mutex<Vec<&'static str>>);

impl ServerProbe for Probe {
    fn checkout(
        &self,
        _: &ServerSeat,
        _: &str,
        _: &dyn ProgressSink,
    ) -> Result<Checkout, ProjectError> {
        let mut all = self.0.lock().unwrap();
        let commit = if all.len() > 1 { all.remove(0) } else { all[0] };
        Ok(Checkout::Repository {
            origin: Some("git@github.com:O/api.git".into()),
            branch: Some("main".into()),
            commit: Some(commit.into()),
            facts: Default::default(),
        })
    }
}

/// 한 줄로 된 커밋 역사 — 앞의 것이 조상. 로컬 main 과 origin/main 이 가리키는 자리를 정한다.
struct History {
    commits: Vec<&'static str>,
    local: &'static str,
    remote: &'static str,
    fetch_fails: bool,
}

impl History {
    fn at(&self, reference: &str) -> Option<usize> {
        let sha = match reference {
            "refs/heads/main" => self.local,
            "refs/remotes/origin/main" => self.remote,
            other => self
                .commits
                .iter()
                .copied()
                .find(|c| c.starts_with(other))?,
        };
        self.commits.iter().position(|c| *c == sha)
    }
}

impl LocalRevisions for History {
    fn fetch(&self, _: &str, _: &str, _: &dyn ProgressSink) -> Result<(), ProjectError> {
        if self.fetch_fails {
            Err(ProjectError::Storage("offline".into()))
        } else {
            Ok(())
        }
    }
    fn resolve(&self, _: &str, reference: &str) -> Option<Revision> {
        let i = self.at(reference)?;
        Some(Revision {
            sha: self.commits[i].into(),
            subject: format!("commit {i}"),
        })
    }
    fn count(&self, _: &str, from: &str, to: &str) -> Option<u32> {
        let (a, b) = (self.at(from)?, self.at(to)?);
        Some(b.saturating_sub(a) as u32)
    }
}

fn line(local: &'static str, remote: &'static str) -> History {
    History {
        commits: vec!["aaaa1111", "bbbb2222", "cccc3333"],
        local,
        remote,
        fetch_fails: false,
    }
}

struct LocalEnv(&'static str);

impl LocalEnvFiles for LocalEnv {
    fn salt(&self) -> String {
        "s".into()
    }
    fn digest(&self, _: &str, _: &str, _: &str) -> Result<EnvDigest, ProjectError> {
        Ok(EnvDigest {
            file: self.0.into(),
            keys: BTreeMap::new(),
        })
    }
    fn read(&self, _: &str, _: &str) -> Result<Vec<u8>, ProjectError> {
        unreachable!("배포는 환경 변수를 올리지 않는다")
    }
}

struct ServerEnvFile(&'static str);

impl ServerEnvFiles for ServerEnvFile {
    fn digest(
        &self,
        _: &ServerSeat,
        _: &str,
        _: &str,
        _: &str,
        _: &dyn ProgressSink,
    ) -> Result<ServerEnv, ProjectError> {
        Ok(ServerEnv::Present {
            digest: EnvDigest {
                file: self.0.into(),
                keys: BTreeMap::new(),
            },
            mode: None,
            owner: None,
            tracking: RepoTracking::Ignored,
        })
    }
    fn write(
        &self,
        _: &ServerSeat,
        _: &str,
        _: &str,
        _: &[u8],
        _: &dyn ProgressSink,
    ) -> Result<(), ProjectError> {
        unreachable!()
    }
}

/// 이 환경에 있는 스크립트 — (이름, 내용).
struct Scripts(Vec<(&'static str, &'static str)>);

impl DeployScripts for Scripts {
    fn names(&self, _: &str, _: &str) -> Result<Vec<String>, ProjectError> {
        Ok(self.0.iter().map(|(n, _)| n.to_string()).collect())
    }
    fn location(&self, _: &str, _: &str, script: &str) -> String {
        format!("/v/projects/api/deploy/prod/{script}.sh")
    }
    fn load(&self, _: &str, _: &str, script: &str) -> Result<Option<String>, ProjectError> {
        Ok(self
            .0
            .iter()
            .find(|(n, _)| *n == script)
            .map(|(_, t)| t.to_string()))
    }
    fn save(&self, _: &str, _: &str, _: &str, _: &str) -> Result<Option<String>, ProjectError> {
        unreachable!()
    }
    fn remove(&self, _: &str, _: &str, _: &str) -> Result<Option<String>, ProjectError> {
        unreachable!()
    }
}

/// 넘겨 받은 변수와 스크립트.
type Ran = (Vec<(String, String)>, String);

#[derive(Default)]
struct Runner {
    ran: Mutex<Vec<Ran>>,
    fails: bool,
}

impl DeployRunner for Runner {
    fn run(
        &self,
        _: &ServerSeat,
        variables: &[(&str, String)],
        script: &str,
        _: &dyn ProgressSink,
    ) -> Result<(), ProjectError> {
        let vars = variables
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        self.ran.lock().unwrap().push((vars, script.into()));
        if self.fails {
            Err(ProjectError::Storage("health 확인 실패".into()))
        } else {
            Ok(())
        }
    }
}

struct World {
    store: Store,
    disk: Disk,
    probe: Probe,
    history: History,
    local_env: LocalEnv,
    server_env: ServerEnvFile,
    scripts: Scripts,
    runner: Runner,
}

impl World {
    /// 로컬 · 원격 · 서버가 모두 cccc3333, 환경 변수 같음, 스크립트 있음.
    fn in_sync() -> World {
        World {
            store: Store {
                env_file: Some(".env.prod"),
            },
            disk: Disk {
                branch: "main",
                changes: 0,
            },
            probe: Probe(Mutex::new(vec!["cccc333"])),
            history: line("cccc3333", "cccc3333"),
            local_env: LocalEnv("same"),
            server_env: ServerEnvFile("same"),
            scripts: Scripts(vec![
                ("deploy", "echo deploy\n"),
                ("migrate", "echo migrate\n"),
            ]),
            runner: Runner::default(),
        }
    }
    fn with<T>(&self, body: impl FnOnce(&Deployer) -> T) -> T {
        let env = EnvSync::new(
            &self.store,
            &self.disk,
            &Seats,
            &self.local_env,
            &self.server_env,
        );
        let deployer = Deployer::new(
            &self.store,
            &self.disk,
            &Seats,
            &self.probe,
            &self.history,
            &env,
            &self.scripts,
            &self.runner,
        );
        body(&deployer)
    }
}

mod plan {
    use super::*;

    #[test]
    fn says_the_same_when_local_remote_and_server_agree() {
        let world = World::in_sync();
        let plan = world
            .with(|d| d.plan("api", "prod", Some("deploy"), &Silent))
            .unwrap();
        assert!(plan.same);
        assert!(plan.notes.is_empty(), "{:?}", plan.notes);
        assert!(plan.blockers.is_empty());
        assert_eq!(plan.incoming, Some(0));
    }

    #[test]
    fn warns_about_unpushed_uncommitted_and_other_branch_but_does_not_block() {
        let world = World {
            disk: Disk {
                branch: "feature",
                changes: 2,
            },
            history: line("cccc3333", "bbbb2222"),
            probe: Probe(Mutex::new(vec!["aaaa111"])),
            ..World::in_sync()
        };
        let plan = world
            .with(|d| d.plan("api", "prod", Some("deploy"), &Silent))
            .unwrap();

        assert!(!plan.same);
        assert!(
            plan.notes
                .contains(&CodeNote::OtherBranch("feature".into()))
        );
        assert!(plan.notes.contains(&CodeNote::Uncommitted(2)));
        assert!(plan.notes.contains(&CodeNote::LocalAhead(1)));
        assert_eq!(plan.incoming, Some(1), "서버 aaaa → 원격 bbbb");
        assert!(plan.blockers.is_empty(), "코드가 달라도 막지 않는다");
    }

    #[test]
    fn warns_when_the_local_branch_is_behind_or_the_server_is_ahead() {
        let world = World {
            history: line("aaaa1111", "bbbb2222"),
            probe: Probe(Mutex::new(vec!["cccc333"])),
            ..World::in_sync()
        };
        let plan = world
            .with(|d| d.plan("api", "prod", Some("deploy"), &Silent))
            .unwrap();
        assert!(plan.notes.contains(&CodeNote::LocalBehind(1)));
        assert!(plan.notes.contains(&CodeNote::ServerAhead(1)));
        assert!(plan.blockers.is_empty());
    }

    #[test]
    fn a_failed_fetch_is_a_note_not_an_error() {
        let world = World {
            history: History {
                fetch_fails: true,
                ..line("cccc3333", "cccc3333")
            },
            ..World::in_sync()
        };
        let plan = world
            .with(|d| d.plan("api", "prod", Some("deploy"), &Silent))
            .unwrap();
        assert!(matches!(plan.notes[0], CodeNote::FetchFailed(_)));
    }

    #[test]
    fn blocks_only_on_a_missing_script_or_a_different_env_file() {
        let world = World {
            scripts: Scripts(vec![]),
            server_env: ServerEnvFile("other"),
            ..World::in_sync()
        };
        let plan = world
            .with(|d| d.plan("api", "prod", Some("deploy"), &Silent))
            .unwrap();
        assert_eq!(plan.blockers, vec![Blocker::NoScript, Blocker::EnvDiffers]);
    }

    #[test]
    fn an_unchosen_script_blocks_like_a_missing_one() {
        let world = World::in_sync();
        let plan = world
            .with(|d| d.plan("api", "prod", None, &Silent))
            .unwrap();
        assert_eq!(plan.blockers, vec![Blocker::NoScript]);

        let plan = world
            .with(|d| d.plan("api", "prod", Some("migrate"), &Silent))
            .unwrap();
        assert_eq!(plan.script_name.as_deref(), Some("migrate"));
        assert_eq!(
            plan.script.as_deref(),
            Some("/v/projects/api/deploy/prod/migrate.sh")
        );
    }

    #[test]
    fn an_unchosen_env_file_is_a_note() {
        let world = World {
            store: Store { env_file: None },
            ..World::in_sync()
        };
        let plan = world
            .with(|d| d.plan("api", "prod", Some("deploy"), &Silent))
            .unwrap();
        assert!(plan.notes.contains(&CodeNote::EnvNotChosen));
        assert!(plan.blockers.is_empty());
    }
}

mod run {
    use super::*;

    #[test]
    fn runs_the_script_with_the_environment_and_reports_the_server_commit_before_and_after() {
        let world = World {
            probe: Probe(Mutex::new(vec!["aaaa111", "cccc333"])),
            ..World::in_sync()
        };
        let done = world
            .with(|d| d.run("api", "prod", "deploy", &Silent))
            .unwrap();

        assert_eq!(done.before.unwrap().sha, "aaaa1111");
        assert_eq!(done.after.unwrap().sha, "cccc3333");
        let ran = world.runner.ran.lock().unwrap();
        assert_eq!(ran[0].1, "echo deploy\n");
        let vars: BTreeMap<_, _> = ran[0].0.iter().cloned().collect();
        assert_eq!(vars["DEPLOY_PATH"], "/srv/api");
        assert_eq!(vars["DEPLOY_BRANCH"], "main");
        assert_eq!(vars["DEPLOY_ENV"], "prod");
        assert_eq!(vars["DEPLOY_ENV_FILE"], "/srv/api/.env.local");
    }

    #[test]
    fn runs_only_the_chosen_script() {
        let world = World::in_sync();
        world
            .with(|d| d.run("api", "prod", "migrate", &Silent))
            .unwrap();
        let ran = world.runner.ran.lock().unwrap();
        assert_eq!(ran.len(), 1);
        assert_eq!(ran[0].1, "echo migrate\n");
        assert!(
            world
                .with(|d| d.run("api", "prod", "../x", &Silent))
                .is_err()
        );
    }

    #[test]
    fn deploys_even_when_the_code_differs() {
        let world = World {
            disk: Disk {
                branch: "feature",
                changes: 5,
            },
            history: line("cccc3333", "aaaa1111"),
            ..World::in_sync()
        };
        assert!(
            world
                .with(|d| d.run("api", "prod", "deploy", &Silent))
                .is_ok()
        );
        assert_eq!(world.runner.ran.lock().unwrap().len(), 1);
    }

    #[test]
    fn refuses_without_a_script_or_with_a_different_env_file() {
        let no_script = World {
            scripts: Scripts(vec![]),
            ..World::in_sync()
        };
        assert!(
            no_script
                .with(|d| d.run("api", "prod", "deploy", &Silent))
                .is_err()
        );

        let env_differs = World {
            server_env: ServerEnvFile("other"),
            ..World::in_sync()
        };
        assert!(
            env_differs
                .with(|d| d.run("api", "prod", "deploy", &Silent))
                .is_err()
        );
        assert!(env_differs.runner.ran.lock().unwrap().is_empty());
    }

    #[test]
    fn a_failing_script_is_a_failed_deploy() {
        let world = World {
            runner: Runner {
                fails: true,
                ..Runner::default()
            },
            ..World::in_sync()
        };
        let err = world
            .with(|d| d.run("api", "prod", "deploy", &Silent))
            .unwrap_err();
        assert!(err.to_string().contains("health 확인 실패"));
    }
}
