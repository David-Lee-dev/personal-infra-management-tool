//! 서버 연결이 무엇을 기록하고, 서버를 읽기 전후에 무엇을 거절하는가.

use std::sync::Mutex;

use secrets_core::port::{Clock, ProgressSink, Silent};
use secrets_core::project::{
    Checkout, GitState, LocalScan, Origin, PathState, ProjectError, ProjectRecord, ProjectStore,
    ServerLink, ServerProbe, ServerRequest, ServerSeat, ServerSeats, Workspace,
};

struct Frozen;

impl Clock for Frozen {
    fn now(&self) -> String {
        "2026-09-24T10:00:00+09:00".into()
    }
    fn today(&self) -> String {
        "2026-09-24".into()
    }
}

struct Store(Mutex<ProjectRecord>);

impl Store {
    fn new() -> Store {
        Store(Mutex::new(ProjectRecord {
            name: "api".into(),
            group: "tuk".into(),
            path: "/w/api".into(),
            origin: Origin::Registered,
            created_at: "2026-09-01T00:00:00+09:00".into(),
            environments: Vec::new(),
        }))
    }
    fn saved(&self) -> ProjectRecord {
        self.0.lock().unwrap().clone()
    }
}

impl ProjectStore for Store {
    fn list(&self) -> Vec<Result<ProjectRecord, String>> {
        vec![Ok(self.saved())]
    }
    fn load(&self, name: &str) -> Result<ProjectRecord, ProjectError> {
        if name != "api" {
            return Err(ProjectError::Missing(name.into()));
        }
        Ok(self.saved())
    }
    fn insert(&self, _: &ProjectRecord) -> Result<(), ProjectError> {
        unreachable!()
    }
    fn replace(&self, record: &ProjectRecord) -> Result<(), ProjectError> {
        *self.0.lock().unwrap() = record.clone();
        Ok(())
    }
}

struct Disk(GitState);

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
            git: self.0.clone(),
            runtimes: Vec::new(),
            env_files: Vec::new(),
            ssh_key: None,
        })
    }
    fn add_to_gitignore(&self, _: &str, _: &[String]) -> Result<(), ProjectError> {
        unreachable!()
    }
}

fn remote() -> GitState {
    GitState::Remote {
        branch: Some("main".into()),
        commits: 5,
        changes: 0,
        origin: "git@github.com:Org/api.git".into(),
    }
}

fn seat(login: &str, admin: bool, verified: bool) -> ServerSeat {
    ServerSeat {
        aws_account: "123".into(),
        machine: "ec2".into(),
        region: "ap-northeast-2".into(),
        keypair: "web-key".into(),
        instance: "i-0abc".into(),
        instance_name: "web-prod".into(),
        address: "3.3.3.3".into(),
        login: login.into(),
        admin,
        verified,
    }
}

struct Seats(Vec<ServerSeat>);

impl ServerSeats for Seats {
    fn seats(&self) -> Vec<ServerSeat> {
        self.0.clone()
    }
}

struct Probe {
    answer: Result<Checkout, String>,
    calls: Mutex<Vec<String>>,
}

impl Probe {
    fn answering(checkout: Checkout) -> Probe {
        Probe {
            answer: Ok(checkout),
            calls: Mutex::new(Vec::new()),
        }
    }
    fn called(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

impl ServerProbe for Probe {
    fn checkout(
        &self,
        seat: &ServerSeat,
        path: &str,
        _: &dyn ProgressSink,
    ) -> Result<Checkout, ProjectError> {
        self.calls.lock().unwrap().push(format!("{} {path}", seat.slug()));
        self.answer.clone().map_err(ProjectError::Storage)
    }
}

fn request(env: &str, seat: &str) -> ServerRequest {
    ServerRequest {
        environment: env.into(),
        seat: seat.into(),
    }
}

fn deploy_seats() -> Seats {
    Seats(vec![seat("deploy", false, true), seat("admin", true, true), seat("fresh", false, false)])
}

fn attach(
    store: &Store,
    git: GitState,
    probe: &Probe,
    request: &ServerRequest,
) -> Result<secrets_core::project::Attached, ProjectError> {
    ServerLink::new(store, &Disk(git), &deploy_seats(), probe, &Frozen).attach("api", request, &Silent)
}

mod attach {
    use super::*;

    #[test]
    fn a_verified_deploy_account_on_an_empty_path_is_recorded() {
        let store = Store::new();
        let probe = Probe::answering(Checkout::Missing);

        let attached = attach(&store, remote(), &probe, &request("prod", "i-0abc/deploy")).unwrap();

        assert_eq!(attached.checkout, Checkout::Missing);
        let env = &store.saved().environments[0];
        assert_eq!(env.name, "prod");
        assert_eq!(env.login, "deploy");
        assert_eq!(env.path, "/srv/api", "배포 경로는 /srv/<레포 이름> 규칙으로 정해진다");
        assert_eq!(env.instance_name, "web-prod");
        assert_eq!(env.connected_at, "2026-09-24T10:00:00+09:00");
    }

    #[test]
    fn a_checkout_of_the_same_repository_is_accepted_whatever_the_case_or_url_shape() {
        let store = Store::new();
        let probe = Probe::answering(Checkout::Repository {
            origin: Some("https://github.com/org/API".into()),
            branch: Some("main".into()),
            commit: Some("38d8077".into()),
        });
        assert!(attach(&store, remote(), &probe, &request("prod", "i-0abc/deploy")).is_ok());
    }

    #[test]
    fn a_checkout_of_another_repository_is_refused_and_not_recorded() {
        let store = Store::new();
        let probe = Probe::answering(Checkout::Repository {
            origin: Some("git@github.com:Org/other.git".into()),
            branch: None,
            commit: None,
        });

        let result = attach(&store, remote(), &probe, &request("prod", "i-0abc/deploy"));

        assert!(matches!(result, Err(ProjectError::Invalid(ref m)) if m.contains("Org/other")));
        assert!(store.saved().environments.is_empty());
    }

    #[test]
    fn an_existing_empty_directory_is_accepted() {
        let store = Store::new();
        let probe = Probe::answering(Checkout::Empty);
        assert!(attach(&store, remote(), &probe, &request("prod", "i-0abc/deploy")).is_ok());
    }

    #[test]
    fn a_plain_directory_is_refused() {
        let store = Store::new();
        let probe = Probe::answering(Checkout::Plain);
        assert!(attach(&store, remote(), &probe, &request("prod", "i-0abc/deploy")).is_err());
        assert!(store.saved().environments.is_empty());
    }
}

mod refuses_before_reading_the_server {
    use super::*;

    fn refused(git: GitState, request: &ServerRequest) {
        let store = Store::new();
        let probe = Probe::answering(Checkout::Missing);
        assert!(attach(&store, git, &probe, request).is_err());
        assert_eq!(probe.called(), 0);
        assert!(store.saved().environments.is_empty());
    }

    #[test]
    fn an_admin_account() {
        refused(remote(), &request("prod", "i-0abc/admin"));
    }

    #[test]
    fn an_account_that_was_never_verified() {
        refused(remote(), &request("prod", "i-0abc/fresh"));
    }

    #[test]
    fn a_project_without_a_github_origin() {
        let local = GitState::Local {
            branch: Some("main".into()),
            commits: 1,
            changes: 0,
        };
        refused(local, &request("prod", "i-0abc/deploy"));
    }

    #[test]
    fn an_unknown_account() {
        refused(remote(), &request("prod", "i-0abc/nobody"));
    }

    #[test]
    fn a_reserved_environment_name() {
        refused(remote(), &request("local", "i-0abc/deploy"));
    }

    #[test]
    fn an_environment_that_is_already_connected() {
        let store = Store::new();
        let probe = Probe::answering(Checkout::Missing);
        attach(&store, remote(), &probe, &request("prod", "i-0abc/deploy")).unwrap();

        let again = attach(&store, remote(), &probe, &request("prod", "i-0abc/deploy"));

        assert!(again.is_err());
        assert_eq!(probe.called(), 1);
        assert_eq!(store.saved().environments.len(), 1);
    }
}

mod check {
    use super::*;

    #[test]
    fn reads_the_recorded_path_with_the_recorded_account() {
        let store = Store::new();
        let probe = Probe::answering(Checkout::Missing);
        attach(&store, remote(), &probe, &request("prod", "i-0abc/deploy")).unwrap();

        let (disk, seats) = (Disk(remote()), deploy_seats());
        let link = ServerLink::new(&store, &disk, &seats, &probe, &Frozen);
        link.check("api", "prod", &Silent).unwrap();

        assert_eq!(probe.calls.lock().unwrap().last().unwrap(), "i-0abc/deploy /srv/api");
        assert!(link.check("api", "dev", &Silent).is_err());
    }
}
