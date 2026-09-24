//! Git 연결이 무엇을 어떤 순서로 하고, 원격에 손대기 전에 무엇을 거절하는가.

use std::sync::Mutex;

use secrets_core::key::RepoRef;
use secrets_core::port::{ProgressSink, Silent};
use secrets_core::project::{
    EnvFileFact, GitLink, GitRequest, GitState, KeyChoice, LocalRepository, LocalScan, Origin,
    PathState, ProjectError, ProjectRecord, ProjectStore, RemoteChoice, RemoteRepos, RepoKey,
    RepoKeys, Visibility, Workspace,
};

const PATH: &str = "/w/ledger";

/// 모든 가짜가 같은 기록장에 적는다. 순서를 보기 위해서다.
#[derive(Default)]
struct Log(Mutex<Vec<String>>);

impl Log {
    fn note(&self, what: impl Into<String>) {
        self.0.lock().unwrap().push(what.into());
    }
    fn all(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

struct Store;

impl ProjectStore for Store {
    fn list(&self) -> Vec<Result<ProjectRecord, String>> {
        vec![Ok(self.load("ledger").unwrap())]
    }
    fn load(&self, name: &str) -> Result<ProjectRecord, ProjectError> {
        if name != "ledger" {
            return Err(ProjectError::Missing(name.into()));
        }
        Ok(ProjectRecord {
            name: "ledger".into(),
            group: "개인".into(),
            path: PATH.into(),
            origin: Origin::Created,
            created_at: "2026-09-24T10:00:00+09:00".into(),
        })
    }
    fn insert(&self, _: &ProjectRecord) -> Result<(), ProjectError> {
        unreachable!("Git 연결은 기록을 새로 쓰지 않는다")
    }
}

struct Disk<'a> {
    log: &'a Log,
    scan: LocalScan,
}

impl Workspace for Disk<'_> {
    fn join(&self, parent: &str, directory: &str) -> String {
        format!("{parent}/{directory}")
    }
    fn state(&self, _: &str) -> PathState {
        PathState::OccupiedDirectory
    }
    fn create_directory(&self, _: &str) -> Result<(), ProjectError> {
        unreachable!()
    }
    fn init_git(&self, path: &str) -> Result<(), ProjectError> {
        self.log.note(format!("git init {path}"));
        Ok(())
    }
    fn scan(&self, _: &str) -> Result<LocalScan, ProjectError> {
        Ok(self.scan.clone())
    }
    fn add_to_gitignore(&self, _: &str, names: &[String]) -> Result<(), ProjectError> {
        self.log.note(format!("gitignore {}", names.join(",")));
        Ok(())
    }
}

struct Keys<'a> {
    log: &'a Log,
    stored: Vec<RepoKey>,
}

fn key(purpose: &str, usable: bool) -> RepoKey {
    RepoKey {
        purpose: purpose.into(),
        account: "david".into(),
        write: true,
        usable,
        private_key: format!("/vault/{purpose}/key"),
    }
}

impl RepoKeys for Keys<'_> {
    fn keys_for(&self, _: &RepoRef) -> Vec<RepoKey> {
        self.stored.clone()
    }
    fn issue(
        &self,
        account: &str,
        repo: &RepoRef,
        purpose: &str,
        _: &dyn ProgressSink,
    ) -> Result<RepoKey, ProjectError> {
        self.log.note(format!("issue {account} {} {purpose}", repo.slug()));
        Ok(key(purpose, true))
    }
}

struct Remotes<'a> {
    log: &'a Log,
    fails: bool,
}

impl RemoteRepos for Remotes<'_> {
    fn create(
        &self,
        account: &str,
        repo: &RepoRef,
        visibility: Visibility,
        _: &dyn ProgressSink,
    ) -> Result<String, ProjectError> {
        self.log.note(format!("create {account} {} {visibility:?}", repo.slug()));
        if self.fails {
            return Err(ProjectError::Storage("GitHub: 이름이 이미 있습니다".into()));
        }
        Ok(format!("git@github.com:{}.git", repo.slug()))
    }
}

struct Local<'a> {
    log: &'a Log,
    unreachable: bool,
}

impl LocalRepository for Local<'_> {
    fn set_origin(&self, _: &str, url: &str) -> Result<(), ProjectError> {
        self.log.note(format!("origin {url}"));
        Ok(())
    }
    fn use_key(&self, _: &str, private_key: &str) -> Result<(), ProjectError> {
        self.log.note(format!("use {private_key}"));
        Ok(())
    }
    fn reach(&self, _: &str, _: &dyn ProgressSink) -> Result<(), ProjectError> {
        self.log.note("ls-remote");
        if self.unreachable {
            return Err(ProjectError::Storage("Permission denied (publickey)".into()));
        }
        Ok(())
    }
}

fn scan(git: GitState) -> LocalScan {
    LocalScan {
        git,
        runtimes: Vec::new(),
        env_files: Vec::new(),
        ssh_key: None,
    }
}

fn local_repo() -> GitState {
    GitState::Local {
        branch: Some("main".into()),
        commits: 2,
        changes: 0,
    }
}

fn remote_repo(origin: &str) -> GitState {
    GitState::Remote {
        branch: Some("main".into()),
        commits: 2,
        changes: 0,
        origin: origin.into(),
    }
}

fn request(remote: RemoteChoice, key: KeyChoice) -> GitRequest {
    GitRequest {
        account: "david".into(),
        remote,
        key,
    }
}

fn create(name: &str) -> RemoteChoice {
    RemoteChoice::Create {
        repo: RepoRef::parse(name).unwrap(),
        visibility: Visibility::Private,
    }
}

fn issue() -> KeyChoice {
    KeyChoice::Issue {
        purpose: "develop".into(),
    }
}

struct World {
    log: Log,
}

impl World {
    fn new() -> World {
        World { log: Log::default() }
    }

    fn connect(
        &self,
        git: GitState,
        stored: Vec<RepoKey>,
        request: &GitRequest,
    ) -> Result<secrets_core::project::Linked, ProjectError> {
        self.connect_with(scan(git), stored, request, false, false)
    }

    fn connect_with(
        &self,
        scan: LocalScan,
        stored: Vec<RepoKey>,
        request: &GitRequest,
        create_fails: bool,
        unreachable: bool,
    ) -> Result<secrets_core::project::Linked, ProjectError> {
        let disk = Disk { log: &self.log, scan };
        let keys = Keys { log: &self.log, stored };
        let remotes = Remotes {
            log: &self.log,
            fails: create_fails,
        };
        let local = Local {
            log: &self.log,
            unreachable,
        };
        GitLink::new(&Store, &disk, &local, &keys, &remotes).connect("ledger", request, &Silent)
    }
}

mod connect {
    use super::*;

    #[test]
    fn a_new_repository_is_created_then_the_key_issued_then_wired_and_checked() {
        let world = World::new();
        let linked = world
            .connect(local_repo(), vec![], &request(create("david/ledger"), issue()))
            .unwrap();

        assert_eq!(
            world.log.all(),
            vec![
                "create david david/ledger Private",
                "origin git@github.com:david/ledger.git",
                "issue david david/ledger develop",
                "use /vault/develop/key",
                "ls-remote",
            ]
        );
        assert!(linked.created_repository && linked.issued_key);
        assert_eq!(linked.unreachable, None);
    }

    #[test]
    fn a_directory_without_git_is_initialised_before_anything_remote() {
        let world = World::new();
        world
            .connect(GitState::Absent, vec![], &request(create("david/ledger"), issue()))
            .unwrap();
        assert_eq!(world.log.all()[0], "git init /w/ledger");
    }

    #[test]
    fn an_existing_origin_with_a_stored_key_only_wires_the_key() {
        let world = World::new();
        let linked = world
            .connect(
                remote_repo("git@github.com:Org/api.git"),
                vec![key("develop", true)],
                &request(RemoteChoice::Current, KeyChoice::Stored { purpose: "develop".into() }),
            )
            .unwrap();

        assert_eq!(world.log.all(), vec!["use /vault/develop/key", "ls-remote"]);
        assert_eq!(linked.repo.slug(), "Org/api");
        assert!(!linked.created_repository && !linked.issued_key);
    }

    #[test]
    fn an_existing_github_repository_becomes_an_ssh_origin() {
        let world = World::new();
        world
            .connect(
                local_repo(),
                vec![],
                &request(RemoteChoice::Existing("https://github.com/Org/api".into()), issue()),
            )
            .unwrap();
        assert_eq!(world.log.all()[0], "origin git@github.com:Org/api.git");
    }

    #[test]
    fn a_failed_check_keeps_the_wiring_and_reports_why() {
        let world = World::new();
        let linked = world
            .connect_with(
                scan(remote_repo("git@github.com:Org/api.git")),
                vec![key("develop", true)],
                &request(RemoteChoice::Current, KeyChoice::Stored { purpose: "develop".into() }),
                false,
                true,
            )
            .unwrap();
        assert!(linked.unreachable.unwrap().contains("publickey"));
    }

    #[test]
    fn a_failed_repository_creation_stops_before_origin_and_key() {
        let world = World::new();
        let result = world.connect_with(
            scan(local_repo()),
            vec![],
            &request(create("david/ledger"), issue()),
            true,
            false,
        );
        assert!(result.is_err());
        assert_eq!(world.log.all(), vec!["create david david/ledger Private"]);
    }
}

mod refuses_before_touching_github {
    use super::*;

    #[test]
    fn when_an_env_file_would_be_pushed() {
        let world = World::new();
        let exposed = LocalScan {
            env_files: vec![EnvFileFact {
                name: ".env.local".into(),
                variables: 3,
                tracked: false,
                ignored: Some(false),
            }],
            ..scan(local_repo())
        };
        let result = world.connect_with(exposed, vec![], &request(create("david/ledger"), issue()), false, false);

        assert!(matches!(result, Err(ProjectError::Invalid(ref m)) if m.contains(".env.local")));
        assert!(world.log.all().is_empty());
    }

    #[test]
    fn when_asked_to_replace_an_existing_origin() {
        let world = World::new();
        let result = world.connect(
            remote_repo("git@github.com:Org/api.git"),
            vec![],
            &request(create("david/other"), issue()),
        );
        assert!(matches!(result, Err(ProjectError::Invalid(_))));
        assert!(world.log.all().is_empty());
    }

    #[test]
    fn when_the_origin_is_not_github() {
        let world = World::new();
        let result = world.connect(
            remote_repo("https://gitlab.com/o/r.git"),
            vec![],
            &request(RemoteChoice::Current, issue()),
        );
        assert!(result.is_err());
        assert!(world.log.all().is_empty());
    }

    #[test]
    fn when_a_key_of_that_purpose_already_exists() {
        let world = World::new();
        let result = world.connect(
            remote_repo("git@github.com:Org/api.git"),
            vec![key("develop", true)],
            &request(RemoteChoice::Current, issue()),
        );
        assert!(matches!(result, Err(ProjectError::Invalid(ref m)) if m.contains("이미")));
        assert!(world.log.all().is_empty());
    }

    #[test]
    fn when_the_stored_key_never_finished_registering() {
        let world = World::new();
        let result = world.connect(
            remote_repo("git@github.com:Org/api.git"),
            vec![key("develop", false)],
            &request(RemoteChoice::Current, KeyChoice::Stored { purpose: "develop".into() }),
        );
        assert!(result.is_err());
        assert!(world.log.all().is_empty());
    }

    #[test]
    fn when_there_is_no_origin_to_keep() {
        let world = World::new();
        let result = world.connect(local_repo(), vec![], &request(RemoteChoice::Current, issue()));
        assert!(result.is_err());
        assert!(world.log.all().is_empty());
    }
}

mod plan {
    use super::*;

    #[test]
    fn lists_the_keys_of_the_origin_repository_and_the_key_in_use() {
        let log = Log::default();
        let disk = Disk {
            log: &log,
            scan: LocalScan {
                ssh_key: Some("/vault/develop/key".into()),
                ..scan(remote_repo("git@github.com:Org/api.git"))
            },
        };
        let keys = Keys {
            log: &log,
            stored: vec![key("develop", true)],
        };
        let remotes = Remotes { log: &log, fails: false };
        let local = Local { log: &log, unreachable: false };

        let plan = GitLink::new(&Store, &disk, &local, &keys, &remotes).plan("ledger").unwrap();

        assert_eq!(plan.repo.unwrap().slug(), "Org/api");
        assert_eq!(plan.keys.len(), 1);
        assert_eq!(plan.current_key.as_deref(), Some("/vault/develop/key"));
        assert!(plan.exposed.is_empty());
    }

    #[test]
    fn without_an_origin_there_is_no_repository_and_no_keys() {
        let log = Log::default();
        let disk = Disk { log: &log, scan: scan(local_repo()) };
        let keys = Keys {
            log: &log,
            stored: vec![key("develop", true)],
        };
        let remotes = Remotes { log: &log, fails: false };
        let local = Local { log: &log, unreachable: false };

        let plan = GitLink::new(&Store, &disk, &local, &keys, &remotes).plan("ledger").unwrap();

        assert!(plan.repo.is_none());
        assert!(plan.keys.is_empty());
    }
}
