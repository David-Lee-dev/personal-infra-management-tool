//! 코드 받기가 서버에 쓰기 전후로 무엇을 확인하고, 무엇을 건드리지 않는가.

use std::sync::Mutex;

use secrets_core::key::RepoRef;
use secrets_core::port::{ProgressSink, Silent};
use secrets_core::project::{
    Checkout, CodePull, Environment, GitState, LocalScan, Origin, PathState, ProjectError,
    ProjectRecord, ProjectStore, RepoKey, RepoKeys, ServerCode, ServerProbe, ServerSeat,
    ServerSeats, Workspace,
};

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
        vec![Ok(self.load("api").unwrap())]
    }
    fn load(&self, name: &str) -> Result<ProjectRecord, ProjectError> {
        if name != "api" {
            return Err(ProjectError::Missing(name.into()));
        }
        Ok(ProjectRecord {
            name: "api".into(),
            group: "tuk".into(),
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
                env_file: None,
                server_env_file: ".env".into(),
            }],
        })
    }
    fn insert(&self, _: &ProjectRecord) -> Result<(), ProjectError> {
        unreachable!()
    }
    fn replace(&self, _: &ProjectRecord) -> Result<(), ProjectError> {
        unreachable!("코드 받기는 기록을 고치지 않는다")
    }
}

struct Disk;

impl Workspace for Disk {
    fn join(&self, parent: &str, directory: &str) -> String {
        format!("{parent}/{directory}")
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
                branch: Some("main".into()),
                commits: 1,
                changes: 0,
                origin: "git@github.com:Org/api.git".into(),
            },
            runtimes: Vec::new(),
            env_files: Vec::new(),
            ssh_key: None,
        })
    }
    fn add_to_gitignore(&self, _: &str, _: &[String]) -> Result<(), ProjectError> {
        unreachable!()
    }
}

struct Seats {
    admin: bool,
    verified: bool,
}

impl ServerSeats for Seats {
    fn seats(&self) -> Vec<ServerSeat> {
        vec![ServerSeat {
            server: "i-1".into(),
            server_name: "web".into(),
            kind: "ec2".into(),
            address: "3.3.3.3".into(),
            port: 22,
            login: "app".into(),
            admin: self.admin,
            verified: self.verified,
            key: Some("/vault/i-1/app/key".into()),
        }]
    }
}

fn ok_seats() -> Seats {
    Seats {
        admin: false,
        verified: true,
    }
}

/// 차례로 답한다 — 쓰기 전, 쓴 뒤.
struct Probe<'a> {
    log: &'a Log,
    answers: Mutex<Vec<Checkout>>,
}

impl ServerProbe for Probe<'_> {
    fn checkout(
        &self,
        _: &ServerSeat,
        path: &str,
        _: &dyn ProgressSink,
    ) -> Result<Checkout, ProjectError> {
        self.log.note(format!("read {path}"));
        Ok(self.answers.lock().unwrap().remove(0))
    }
}

struct Keys {
    stored: Vec<RepoKey>,
}

fn key(purpose: &str, usable: bool) -> RepoKey {
    RepoKey {
        purpose: purpose.into(),
        account: "david".into(),
        write: false,
        usable,
        private_key: format!("/vault/{purpose}/key"),
    }
}

impl RepoKeys for Keys {
    fn keys_for(&self, _: &RepoRef) -> Vec<RepoKey> {
        self.stored.clone()
    }
}

struct Code<'a> {
    log: &'a Log,
}

impl ServerCode for Code<'_> {
    fn clone_repository(
        &self,
        seat: &ServerSeat,
        repo: &RepoRef,
        branch: &str,
        path: &str,
        private_key: &str,
        _: &dyn ProgressSink,
    ) -> Result<(), ProjectError> {
        self.log.note(format!(
            "clone {} {} {branch} {path} {private_key}",
            seat.slug(),
            repo.slug()
        ));
        Ok(())
    }
}

fn same_repo() -> Checkout {
    Checkout::Repository {
        origin: Some("git@github.com:Org/api.git".into()),
        branch: Some("main".into()),
        commit: Some("38d8077".into()),
        facts: Default::default(),
    }
}

fn stored(purpose: &str) -> String {
    purpose.into()
}

fn pull(
    log: &Log,
    seats: Seats,
    answers: Vec<Checkout>,
    stored_keys: Vec<RepoKey>,
    choice: String,
) -> Result<secrets_core::project::Pulled, ProjectError> {
    let probe = Probe {
        log,
        answers: Mutex::new(answers),
    };
    let keys = Keys {
        stored: stored_keys,
    };
    let code = Code { log };
    CodePull::new(&Store, &Disk, &seats, &probe, &keys, &code).pull("api", "prod", &choice, &Silent)
}

mod pull {
    use super::*;

    #[test]
    fn with_a_stored_key_reads_clones_and_reads_again() {
        let log = Log::default();
        let pulled = pull(
            &log,
            ok_seats(),
            vec![Checkout::Missing, same_repo()],
            vec![key("deploy", true)],
            stored("deploy"),
        )
        .unwrap();

        assert_eq!(
            log.all(),
            vec![
                "read /srv/api",
                "clone web/app Org/api main /srv/api /vault/deploy/key",
                "read /srv/api",
            ]
        );
        assert!(!pulled.already);
        assert_eq!(pulled.checkout, same_repo());
    }

    #[test]
    fn an_existing_checkout_of_the_same_repository_is_left_alone() {
        let log = Log::default();
        let pulled = pull(
            &log,
            ok_seats(),
            vec![same_repo()],
            vec![],
            stored("deploy"),
        )
        .unwrap();

        assert!(pulled.already);
        assert_eq!(log.all(), vec!["read /srv/api"]);
    }

    #[test]
    fn a_clone_that_does_not_show_up_afterwards_is_an_error() {
        let log = Log::default();
        let result = pull(
            &log,
            ok_seats(),
            vec![Checkout::Missing, Checkout::Empty],
            vec![key("deploy", true)],
            stored("deploy"),
        );
        assert!(result.is_err());
    }
}

mod does_not_write {
    use super::*;

    fn untouched(answers: Vec<Checkout>, stored_keys: Vec<RepoKey>, choice: String) {
        let log = Log::default();
        assert!(pull(&log, ok_seats(), answers, stored_keys, choice).is_err());
        assert!(
            log.all().iter().all(|l| !l.starts_with("clone")),
            "{:?}",
            log.all()
        );
    }

    #[test]
    fn when_another_repository_is_there() {
        let other = Checkout::Repository {
            origin: Some("git@github.com:Org/other.git".into()),
            branch: None,
            commit: None,
            facts: Default::default(),
        };
        untouched(vec![other], vec![key("deploy", true)], stored("deploy"));
    }

    #[test]
    fn when_plain_files_are_there() {
        untouched(
            vec![Checkout::Plain],
            vec![key("deploy", true)],
            stored("deploy"),
        );
    }

    #[test]
    fn when_the_chosen_key_never_finished_registering() {
        untouched(
            vec![Checkout::Missing],
            vec![key("deploy", false)],
            stored("deploy"),
        );
    }

    #[test]
    fn when_the_chosen_key_does_not_exist() {
        untouched(
            vec![Checkout::Missing],
            vec![key("develop", true)],
            stored("deploy"),
        );
    }
}
