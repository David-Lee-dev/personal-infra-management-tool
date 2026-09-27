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

fn seat_on(instance: &str, login: &str, admin: bool, verified: bool) -> ServerSeat {
    ServerSeat {
        server: instance.into(),
        server_name: format!("{instance}-name"),
        kind: "ec2".into(),
        address: "3.3.3.3".into(),
        port: 22,
        login: login.into(),
        admin,
        verified,
        key: Some(format!("/vault/{instance}/{login}/key")),
    }
}

struct Seats(Vec<ServerSeat>);

impl ServerSeats for Seats {
    fn seats(&self) -> Vec<ServerSeat> {
        self.0.clone()
    }
}

fn seats() -> Seats {
    Seats(vec![
        seat_on("i-web", "ops", true, true),
        seat_on("i-web", "app", false, true),
        seat_on("i-db", "ops", true, true),
    ])
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
        self.calls
            .lock()
            .unwrap()
            .push(format!("{} {path}", seat.slug()));
        self.answer.clone().map_err(ProjectError::Storage)
    }
}

fn request(env: &str, server: &str, login: &str, path: &str, branch: &str) -> ServerRequest {
    ServerRequest {
        environment: env.into(),
        server: server.into(),
        login: login.into(),
        path: path.into(),
        branch: branch.into(),
    }
}

fn web_app() -> ServerRequest {
    request("prod", "i-web", "app", "/opt/api/", "release")
}

fn attach(
    store: &Store,
    git: GitState,
    probe: &Probe,
    request: &ServerRequest,
) -> Result<secrets_core::project::Attached, ProjectError> {
    ServerLink::new(store, &Disk(git), &seats(), probe, &Frozen).attach("api", request, &Silent)
}

fn repository(origin: &str) -> Checkout {
    Checkout::Repository {
        origin: Some(origin.into()),
        branch: Some("main".into()),
        commit: Some("38d8077".into()),
        facts: Default::default(),
    }
}

mod attach {
    use super::*;

    #[test]
    fn records_exactly_what_the_user_chose() {
        let store = Store::new();
        let probe = Probe::answering(Checkout::Missing);

        attach(&store, remote(), &probe, &web_app()).unwrap();

        let env = &store.saved().environments[0];
        assert_eq!((env.name.as_str(), env.login.as_str()), ("prod", "app"));
        assert_eq!(env.path, "/opt/api", "끝의 / 만 정리하고 고른 경로 그대로");
        assert_eq!(env.branch, "release");
        assert_eq!(env.server, "i-web");
        assert_eq!((env.instance.as_ref(), env.address.as_ref()), (None, None));
        assert_eq!(env.connected_at, "2026-09-24T10:00:00+09:00");
    }

    #[test]
    fn any_account_on_the_instance_can_be_chosen_including_one_with_sudo() {
        let store = Store::new();
        let probe = Probe::answering(Checkout::Empty);
        attach(
            &store,
            remote(),
            &probe,
            &request("prod", "i-web", "ops", "/srv/api", "main"),
        )
        .unwrap();
        assert_eq!(store.saved().environments[0].login, "ops");
    }

    #[test]
    fn a_checkout_of_the_same_repository_is_accepted_whatever_the_url_shape() {
        let store = Store::new();
        let probe = Probe::answering(repository("https://github.com/org/API"));
        assert!(attach(&store, remote(), &probe, &web_app()).is_ok());
    }

    #[test]
    fn a_checkout_of_another_repository_is_refused_and_not_recorded() {
        let store = Store::new();
        let probe = Probe::answering(repository("git@github.com:Org/other.git"));
        let result = attach(&store, remote(), &probe, &web_app());
        assert!(matches!(result, Err(ProjectError::Invalid(ref m)) if m.contains("Org/other")));
        assert!(store.saved().environments.is_empty());
    }

    #[test]
    fn a_plain_directory_is_refused() {
        let store = Store::new();
        let probe = Probe::answering(Checkout::Plain);
        assert!(attach(&store, remote(), &probe, &web_app()).is_err());
        assert!(store.saved().environments.is_empty());
    }

    #[test]
    fn a_probe_failure_leaves_nothing_recorded() {
        let store = Store::new();
        let probe = Probe {
            answer: Err("Permission denied (publickey)".into()),
            calls: Mutex::new(Vec::new()),
        };
        assert!(attach(&store, remote(), &probe, &web_app()).is_err());
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
    fn a_project_without_a_github_origin() {
        let local = GitState::Local {
            branch: Some("main".into()),
            commits: 1,
            changes: 0,
        };
        refused(local, &web_app());
    }

    #[test]
    fn a_relative_path() {
        refused(
            remote(),
            &request("prod", "i-web", "app", "srv/api", "main"),
        );
    }

    #[test]
    fn an_empty_branch() {
        refused(remote(), &request("prod", "i-web", "app", "/srv/api", "  "));
    }

    #[test]
    fn an_account_that_is_not_on_that_instance() {
        refused(
            remote(),
            &request("prod", "i-db", "app", "/srv/api", "main"),
        );
    }

    #[test]
    fn a_reserved_environment_name() {
        refused(
            remote(),
            &request("local", "i-web", "app", "/srv/api", "main"),
        );
    }

    #[test]
    fn an_environment_that_is_already_connected() {
        let store = Store::new();
        let probe = Probe::answering(Checkout::Missing);
        attach(&store, remote(), &probe, &web_app()).unwrap();

        let again = attach(
            &store,
            remote(),
            &probe,
            &request("prod", "i-db", "ops", "/srv/other", "main"),
        );

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
        attach(&store, remote(), &probe, &web_app()).unwrap();

        let (disk, seats) = (Disk(remote()), seats());
        let link = ServerLink::new(&store, &disk, &seats, &probe, &Frozen);
        assert_eq!(
            link.check("api", "prod", &Silent).unwrap(),
            Checkout::Missing
        );

        assert_eq!(
            probe.calls.lock().unwrap().last().unwrap(),
            "i-web-name/app /opt/api"
        );
        assert!(link.check("api", "dev", &Silent).is_err());
    }
}

mod servers {
    use super::*;

    #[test]
    fn groups_every_account_under_its_server() {
        let (disk, seats, probe, store) = (
            Disk(remote()),
            seats(),
            Probe::answering(Checkout::Missing),
            Store::new(),
        );
        let link = ServerLink::new(&store, &disk, &seats, &probe, &Frozen);

        let found = link.servers();

        let ids: Vec<&str> = found.iter().map(|s| s.server.as_str()).collect();
        assert_eq!(ids, vec!["i-db", "i-web"]);
        let web: Vec<&str> = found[1].accounts.iter().map(|a| a.login.as_str()).collect();
        assert_eq!(web, vec!["app", "ops"]);
    }
}

mod legacy {
    use super::*;
    use secrets_core::project::Environment;

    fn env(name: &str, server: &str, instance: Option<&str>, address: Option<&str>) -> Environment {
        Environment {
            name: name.into(),
            server: server.into(),
            login: "deploy".into(),
            path: "/srv/api".into(),
            branch: "main".into(),
            connected_at: "t".into(),
            env_file: None,
            server_env_file: ".env".into(),
            instance: instance.map(str::to_string),
            address: address.map(str::to_string),
        }
    }

    fn store_with(environments: Vec<Environment>) -> Store {
        let store = Store::new();
        store.0.lock().unwrap().environments = environments;
        store
    }

    fn link<'a>(
        store: &'a Store,
        disk: &'a Disk,
        seats: &'a Seats,
        probe: &'a Probe,
    ) -> ServerLink<'a> {
        ServerLink::new(store, disk, seats, probe, &Frozen)
    }

    #[test]
    fn old_environments_on_that_instance_or_address_are_linked_and_forget_the_copies() {
        let store = store_with(vec![
            env("prod", "", Some("i-0b97"), Some("54.116.119.214")),
            env("dev", "", Some("i-04bb"), Some("43.202.16.82")),
            env("stage", "", None, Some("54.116.119.214")),
        ]);
        let (disk, seats, probe) = (Disk(remote()), seats(), Probe::answering(Checkout::Missing));
        let link = link(&store, &disk, &seats, &probe);

        assert_eq!(
            link.legacy_users(Some("i-0b97"), "54.116.119.214"),
            vec!["api/prod", "api/stage"]
        );
        let linked = link
            .adopt_legacy("tuk-api-server", Some("i-0b97"), "54.116.119.214")
            .unwrap();

        assert_eq!(linked, vec!["api/prod", "api/stage"]);
        let saved = store.saved();
        assert_eq!(saved.environments[0].server, "tuk-api-server");
        assert_eq!(
            (
                saved.environments[0].instance.as_ref(),
                saved.environments[0].address.as_ref()
            ),
            (None, None)
        );
        assert_eq!(saved.environments[1].server, "", "다른 인스턴스는 그대로");
        assert!(
            link.legacy_users(Some("i-0b97"), "54.116.119.214")
                .is_empty()
        );
    }

    #[test]
    fn an_environment_that_already_points_at_a_server_is_left_alone() {
        let store = store_with(vec![env(
            "prod",
            "other",
            Some("i-0b97"),
            Some("54.116.119.214"),
        )]);
        let (disk, seats, probe) = (Disk(remote()), seats(), Probe::answering(Checkout::Missing));
        let link = link(&store, &disk, &seats, &probe);

        assert!(
            link.adopt_legacy("tuk-api-server", Some("i-0b97"), "x")
                .unwrap()
                .is_empty()
        );
        assert_eq!(store.saved().environments[0].server, "other");
    }

    #[test]
    fn an_unlinked_environment_asks_for_the_server_to_be_registered_first() {
        let store = store_with(vec![env("prod", "", Some("i-web"), Some("3.3.3.3"))]);
        let (disk, seats, probe) = (Disk(remote()), seats(), Probe::answering(Checkout::Missing));
        let link = link(&store, &disk, &seats, &probe);

        assert!(matches!(
            link.check("api", "prod", &Silent),
            Err(ProjectError::Invalid(_))
        ));
        assert!(
            probe.calls.lock().unwrap().is_empty(),
            "서버에 들어가지 않는다"
        );
    }

    #[test]
    fn users_of_a_server_or_one_of_its_accounts() {
        let mut other = env("dev", "i-web", None, None);
        other.login = "app".into();
        let store = store_with(vec![env("prod", "i-web", None, None), other]);
        let (disk, seats, probe) = (Disk(remote()), seats(), Probe::answering(Checkout::Missing));
        let link = link(&store, &disk, &seats, &probe);

        assert_eq!(link.used_by("i-web", None), vec!["api/prod", "api/dev"]);
        assert_eq!(link.used_by("i-web", Some("app")), vec!["api/dev"]);
        assert!(link.used_by("i-db", None).is_empty());
    }
}
