//! 등록한 뒤의 수정과 제거 — 기록만 고치고, 서버 쪽을 바꿀 때는 연결할 때와 같이 검사한다.

use std::sync::Mutex;

use secrets_core::port::{ProgressSink, Silent};
use secrets_core::project::{
    Checkout, Environment, EnvironmentEdit, GitState, LocalScan, Origin, PathState, ProjectEdit,
    ProjectEditor, ProjectError, ProjectFiles, ProjectRecord, ProjectStore, ServerProbe,
    ServerSeat, ServerSeats, Workspace,
};

fn environment(name: &str) -> Environment {
    Environment {
        name: name.into(),
        server: "i-1".into(),
        instance: None,
        address: None,
        login: "deploy".into(),
        path: "/srv/api".into(),
        branch: "main".into(),
        connected_at: "t".into(),
        env_file: Some(".env.prod".into()),
        server_env_file: ".env.prod".into(),
    }
}

fn record(name: &str, path: &str) -> ProjectRecord {
    ProjectRecord {
        name: name.into(),
        group: "g".into(),
        path: path.into(),
        origin: Origin::Registered,
        created_at: "t".into(),
        environments: vec![environment("prod"), environment("dev")],
    }
}

struct Store(Mutex<Vec<ProjectRecord>>);

impl ProjectStore for Store {
    fn list(&self) -> Vec<Result<ProjectRecord, String>> {
        self.0.lock().unwrap().iter().cloned().map(Ok).collect()
    }
    fn load(&self, name: &str) -> Result<ProjectRecord, ProjectError> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.name == name)
            .cloned()
            .ok_or_else(|| ProjectError::Missing(name.into()))
    }
    fn insert(&self, _: &ProjectRecord) -> Result<(), ProjectError> {
        unreachable!()
    }
    fn replace(&self, record: &ProjectRecord) -> Result<(), ProjectError> {
        let mut all = self.0.lock().unwrap();
        let slot = all
            .iter_mut()
            .find(|r| r.name == record.name)
            .ok_or_else(|| ProjectError::Missing(record.name.clone()))?;
        *slot = record.clone();
        Ok(())
    }
}

/// 디렉토리 이름 바꾸기를 기록에도 반영하는 가짜 — 실제로 `projects/<이름>/` 이 옮겨지는 것과 같다.
struct Files<'a> {
    store: &'a Store,
    log: Mutex<Vec<String>>,
}

impl ProjectFiles for Files<'_> {
    fn rename_project(&self, from: &str, to: &str) -> Result<(), ProjectError> {
        self.log.lock().unwrap().push(format!("rename {from} {to}"));
        let mut all = self.store.0.lock().unwrap();
        all.iter_mut().find(|r| r.name == from).unwrap().name = to.into();
        Ok(())
    }
    fn archive_project(&self, name: &str) -> Result<String, ProjectError> {
        self.log.lock().unwrap().push(format!("archive {name}"));
        self.store.0.lock().unwrap().retain(|r| r.name != name);
        Ok(format!("/archive/{name}"))
    }
    fn rename_environment(&self, project: &str, from: &str, to: &str) -> Result<(), ProjectError> {
        self.log
            .lock()
            .unwrap()
            .push(format!("rename-env {project} {from} {to}"));
        Ok(())
    }
    fn archive_environment(
        &self,
        project: &str,
        environment: &str,
        record: &str,
    ) -> Result<String, ProjectError> {
        assert!(
            record.contains("name = \"prod\""),
            "빼는 환경의 기록을 남긴다"
        );
        self.log
            .lock()
            .unwrap()
            .push(format!("archive-env {project} {environment}"));
        Ok(format!("/archive/{project}/{environment}"))
    }
}

struct Disk;

impl Workspace for Disk {
    fn join(&self, a: &str, b: &str) -> String {
        format!("{a}/{b}")
    }
    fn state(&self, path: &str) -> PathState {
        match path {
            "/w/moved" | "/w/api" | "/w/web" => PathState::OccupiedDirectory,
            "/w/file" => PathState::NotDirectory,
            _ => PathState::Missing,
        }
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
        ["deploy", "app"]
            .iter()
            .map(|login| ServerSeat {
                server: "i-2".into(),
                server_name: "api-2".into(),
                kind: "ec2".into(),
                address: "2.2.2.2".into(),
                port: 22,
                login: (*login).into(),
                admin: false,
                verified: true,
                key: Some("/k".into()),
            })
            .collect()
    }
}

struct Probe {
    answer: Checkout,
    reads: Mutex<u32>,
}

impl ServerProbe for Probe {
    fn checkout(
        &self,
        _: &ServerSeat,
        _: &str,
        _: &dyn ProgressSink,
    ) -> Result<Checkout, ProjectError> {
        *self.reads.lock().unwrap() += 1;
        Ok(self.answer.clone())
    }
}

fn probe(answer: Checkout) -> Probe {
    Probe {
        answer,
        reads: Mutex::new(0),
    }
}

fn same_repo() -> Checkout {
    Checkout::Repository {
        origin: Some("git@github.com:O/api.git".into()),
        branch: Some("main".into()),
        commit: Some("abc".into()),
        facts: Default::default(),
    }
}

fn store() -> Store {
    Store(Mutex::new(vec![
        record("api", "/w/api"),
        record("web", "/w/web"),
    ]))
}

fn edit(name: &str, group: &str, path: &str) -> ProjectEdit {
    ProjectEdit {
        name: name.into(),
        group: group.into(),
        path: path.into(),
    }
}

mod project {
    use super::*;

    #[test]
    fn renames_moves_the_record_directory_and_changes_group_and_path() {
        let store = store();
        let files = Files {
            store: &store,
            log: Mutex::default(),
        };
        let p = probe(same_repo());
        let editor = ProjectEditor::new(&store, &files, &Disk, &Seats, &p);

        let done = editor
            .update("api", &edit("api-server", "tuk", "/w/moved"))
            .unwrap();

        assert_eq!(
            (done.name.as_str(), done.group.as_str(), done.path.as_str()),
            ("api-server", "tuk", "/w/moved")
        );
        assert_eq!(*files.log.lock().unwrap(), vec!["rename api api-server"]);
        assert_eq!(store.load("api-server").unwrap().path, "/w/moved");
    }

    #[test]
    fn keeping_the_name_does_not_move_anything() {
        let store = store();
        let files = Files {
            store: &store,
            log: Mutex::default(),
        };
        let p = probe(same_repo());
        let editor = ProjectEditor::new(&store, &files, &Disk, &Seats, &p);
        editor
            .update("api", &edit("api", "other", "/w/api"))
            .unwrap();
        assert!(files.log.lock().unwrap().is_empty());
        assert_eq!(store.load("api").unwrap().group, "other");
    }

    #[test]
    fn refuses_a_taken_name_a_taken_path_and_a_path_that_is_not_a_directory() {
        let store = store();
        let files = Files {
            store: &store,
            log: Mutex::default(),
        };
        let p = probe(same_repo());
        let editor = ProjectEditor::new(&store, &files, &Disk, &Seats, &p);

        assert!(editor.update("api", &edit("web", "g", "/w/api")).is_err());
        assert!(editor.update("api", &edit("api", "g", "/w/web")).is_err());
        assert!(
            editor
                .update("api", &edit("api", "g", "/w/nowhere"))
                .is_err()
        );
        assert!(editor.update("api", &edit("api", "g", "/w/file")).is_err());
        assert!(files.log.lock().unwrap().is_empty());
    }

    #[test]
    fn unregistering_archives_the_record() {
        let store = store();
        let files = Files {
            store: &store,
            log: Mutex::default(),
        };
        let p = probe(same_repo());
        let editor = ProjectEditor::new(&store, &files, &Disk, &Seats, &p);

        assert_eq!(editor.unregister("api").unwrap(), "/archive/api");
        assert!(store.load("api").is_err());
        assert!(editor.unregister("nothing").is_err());
    }
}

mod environment {
    use super::*;

    fn change(name: &str, server: &str, login: &str, path: &str, branch: &str) -> EnvironmentEdit {
        EnvironmentEdit {
            name: name.into(),
            server: server.into(),
            login: login.into(),
            path: path.into(),
            branch: branch.into(),
        }
    }

    #[test]
    fn changing_only_the_branch_does_not_read_the_server() {
        let store = store();
        let files = Files {
            store: &store,
            log: Mutex::default(),
        };
        let p = probe(same_repo());
        let editor = ProjectEditor::new(&store, &files, &Disk, &Seats, &p);

        let done = editor
            .update_environment(
                "api",
                "prod",
                &change("prod", "i-1", "deploy", "/srv/api", "release"),
                &Silent,
            )
            .unwrap();

        assert_eq!(done.environment.branch, "release");
        assert!(done.checkout.is_none());
        assert_eq!(*p.reads.lock().unwrap(), 0);
        let kept = store.load("api").unwrap();
        assert_eq!(
            kept.environments[0].env_file.as_deref(),
            Some(".env.prod"),
            "고치지 않은 값은 그대로"
        );
    }

    #[test]
    fn moving_to_another_account_reads_the_server_and_takes_the_seat() {
        let store = store();
        let files = Files {
            store: &store,
            log: Mutex::default(),
        };
        let p = probe(Checkout::Empty);
        let editor = ProjectEditor::new(&store, &files, &Disk, &Seats, &p);

        let done = editor
            .update_environment(
                "api",
                "prod",
                &change("prod", "i-2", "app", "/srv/api2", "main"),
                &Silent,
            )
            .unwrap();

        assert_eq!(*p.reads.lock().unwrap(), 1);
        assert_eq!(done.checkout, Some(Checkout::Empty));
        let env = &store.load("api").unwrap().environments[0];
        assert_eq!((env.server.as_str(), env.login.as_str()), ("i-2", "app"));
        assert_eq!(env.path, "/srv/api2");
    }

    #[test]
    fn refuses_a_path_holding_another_repository_or_plain_files() {
        let store = store();
        let files = Files {
            store: &store,
            log: Mutex::default(),
        };
        let other = probe(Checkout::Repository {
            origin: Some("git@github.com:O/else.git".into()),
            branch: None,
            commit: None,
            facts: Default::default(),
        });
        let editor = ProjectEditor::new(&store, &files, &Disk, &Seats, &other);
        assert!(
            editor
                .update_environment(
                    "api",
                    "prod",
                    &change("prod", "i-2", "app", "/srv/x", "main"),
                    &Silent
                )
                .is_err()
        );

        let plain = probe(Checkout::Plain);
        let editor = ProjectEditor::new(&store, &files, &Disk, &Seats, &plain);
        assert!(
            editor
                .update_environment(
                    "api",
                    "prod",
                    &change("prod", "i-1", "deploy", "/srv/x", "main"),
                    &Silent
                )
                .is_err()
        );
        assert_eq!(
            store.load("api").unwrap().environments[0].path,
            "/srv/api",
            "거부하면 기록은 그대로"
        );
    }

    #[test]
    fn renaming_moves_the_deploy_scripts_and_refuses_a_taken_or_bad_name() {
        let store = store();
        let files = Files {
            store: &store,
            log: Mutex::default(),
        };
        let p = probe(same_repo());
        let editor = ProjectEditor::new(&store, &files, &Disk, &Seats, &p);

        assert!(
            editor
                .update_environment(
                    "api",
                    "prod",
                    &change("dev", "i-1", "deploy", "/srv/api", "main"),
                    &Silent
                )
                .is_err()
        );
        assert!(
            editor
                .update_environment(
                    "api",
                    "prod",
                    &change("local", "i-1", "deploy", "/srv/api", "main"),
                    &Silent
                )
                .is_err()
        );
        editor
            .update_environment(
                "api",
                "prod",
                &change("live", "i-1", "deploy", "/srv/api", "main"),
                &Silent,
            )
            .unwrap();

        assert_eq!(*files.log.lock().unwrap(), vec!["rename-env api prod live"]);
        let names: Vec<_> = store
            .load("api")
            .unwrap()
            .environments
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, vec!["live", "dev"]);
    }

    #[test]
    fn removing_archives_the_environment_and_keeps_the_others() {
        let store = store();
        let files = Files {
            store: &store,
            log: Mutex::default(),
        };
        let p = probe(same_repo());
        let editor = ProjectEditor::new(&store, &files, &Disk, &Seats, &p);

        assert_eq!(
            editor.remove_environment("api", "prod").unwrap(),
            "/archive/api/prod"
        );
        let names: Vec<_> = store
            .load("api")
            .unwrap()
            .environments
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, vec!["dev"]);
        assert_eq!(*p.reads.lock().unwrap(), 0, "빼기는 서버를 건드리지 않는다");
    }
}
