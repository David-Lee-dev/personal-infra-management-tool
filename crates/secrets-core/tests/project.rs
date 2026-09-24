//! 프로젝트를 만들고 등록할 때 무엇이 어떤 순서로 생기는가, 그리고 무엇을 거절하는가.

use std::collections::BTreeMap;
use std::sync::Mutex;

use secrets_core::port::Clock;
use secrets_core::project::{
    EnvFileFact, GitStart, GitState, LocalScan, NewProject, Origin, PathState, ProjectError,
    ProjectRecord, ProjectStore, Projects, Registration, StageState, Workspace,
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

/* ── 가짜 기록 ──────────────────────────────────────── */

#[derive(Default)]
struct Store {
    records: Mutex<BTreeMap<String, ProjectRecord>>,
    unreadable: Vec<String>,
    log: Option<&'static Mutex<Vec<String>>>,
}

impl ProjectStore for Store {
    fn list(&self) -> Vec<Result<ProjectRecord, String>> {
        let mut all: Vec<Result<ProjectRecord, String>> =
            self.records.lock().unwrap().values().cloned().map(Ok).collect();
        all.extend(self.unreadable.iter().cloned().map(Err));
        all
    }

    fn load(&self, name: &str) -> Result<ProjectRecord, ProjectError> {
        self.records
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .ok_or_else(|| ProjectError::Missing(name.into()))
    }

    fn insert(&self, record: &ProjectRecord) -> Result<(), ProjectError> {
        if let Some(log) = self.log {
            log.lock().unwrap().push(format!("record {}", record.name));
        }
        let mut records = self.records.lock().unwrap();
        if records.contains_key(&record.name) {
            return Err(ProjectError::Taken(record.name.clone()));
        }
        records.insert(record.name.clone(), record.clone());
        Ok(())
    }
}

/* ── 가짜 작업 공간 ─────────────────────────────────── */

#[derive(Default)]
struct Disk {
    /// 경로 → 비어 있는가.
    dirs: Mutex<BTreeMap<String, bool>>,
    scans: BTreeMap<String, LocalScan>,
    git_fails: bool,
    log: Option<&'static Mutex<Vec<String>>>,
}

impl Disk {
    fn with(dirs: &[(&str, bool)]) -> Disk {
        Disk {
            dirs: Mutex::new(dirs.iter().map(|(p, e)| (p.to_string(), *e)).collect()),
            ..Disk::default()
        }
    }

    fn note(&self, what: String) {
        if let Some(log) = self.log {
            log.lock().unwrap().push(what);
        }
    }
}

fn plain_scan() -> LocalScan {
    LocalScan {
        git: GitState::Absent,
        runtimes: Vec::new(),
        env_files: Vec::new(),
        ssh_key: None,
    }
}

impl Workspace for Disk {
    fn join(&self, parent: &str, directory: &str) -> String {
        format!("{parent}/{directory}")
    }

    fn state(&self, path: &str) -> PathState {
        match self.dirs.lock().unwrap().get(path) {
            None => PathState::Missing,
            Some(true) => PathState::EmptyDirectory,
            Some(false) => PathState::OccupiedDirectory,
        }
    }

    fn create_directory(&self, path: &str) -> Result<(), ProjectError> {
        self.note(format!("mkdir {path}"));
        self.dirs.lock().unwrap().insert(path.to_string(), true);
        Ok(())
    }

    fn init_git(&self, path: &str) -> Result<(), ProjectError> {
        self.note(format!("git init {path}"));
        if self.git_fails {
            return Err(ProjectError::Storage("git 을 찾을 수 없습니다".into()));
        }
        self.dirs.lock().unwrap().insert(path.to_string(), false);
        Ok(())
    }

    fn scan(&self, path: &str) -> Result<LocalScan, ProjectError> {
        Ok(self.scans.get(path).cloned().unwrap_or_else(plain_scan))
    }

    fn add_to_gitignore(&self, path: &str, names: &[String]) -> Result<(), ProjectError> {
        self.note(format!("gitignore {path} {}", names.join(",")));
        Ok(())
    }
}

fn new_project(name: &str, parent: &str, directory: &str) -> NewProject {
    NewProject {
        name: name.into(),
        group: "개인".into(),
        parent: parent.into(),
        directory: directory.into(),
        git: GitStart::Init,
    }
}

fn registration(name: &str, path: &str) -> Registration {
    Registration {
        name: name.into(),
        group: "tuk".into(),
        path: path.into(),
    }
}

fn record(name: &str, group: &str, path: &str) -> ProjectRecord {
    ProjectRecord {
        name: name.into(),
        group: group.into(),
        path: path.into(),
        origin: Origin::Registered,
        created_at: "2026-09-01T00:00:00+09:00".into(),
    }
}

mod create {
    use super::*;

    #[test]
    fn makes_the_directory_then_the_record_then_git() {
        static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let store = Store {
            log: Some(&LOG),
            ..Store::default()
        };
        let disk = Disk {
            log: Some(&LOG),
            ..Disk::with(&[("/w", false)])
        };

        let created = Projects::new(&store, &disk, &Frozen)
            .create(&new_project("ledger", "/w", "09_ledger"))
            .unwrap();

        assert_eq!(
            *LOG.lock().unwrap(),
            vec!["mkdir /w/09_ledger", "record ledger", "git init /w/09_ledger"]
        );
        assert_eq!(created.record.path, "/w/09_ledger");
        assert_eq!(created.record.origin, Origin::Created);
        assert_eq!(created.record.created_at, "2026-09-24T10:00:00+09:00");
        assert!(created.incomplete.is_empty());
    }

    #[test]
    fn a_failed_git_init_keeps_the_project_and_reports_it() {
        let store = Store::default();
        let disk = Disk {
            git_fails: true,
            ..Disk::with(&[("/w", false)])
        };

        let created = Projects::new(&store, &disk, &Frozen)
            .create(&new_project("ledger", "/w", "ledger"))
            .unwrap();

        assert_eq!(created.incomplete.len(), 1);
        assert!(store.load("ledger").is_ok());
        assert_eq!(disk.state("/w/ledger"), PathState::EmptyDirectory);
    }

    #[test]
    fn without_git_only_the_directory_is_made() {
        static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let disk = Disk {
            log: Some(&LOG),
            ..Disk::with(&[("/w", false)])
        };
        let request = NewProject {
            git: GitStart::None,
            ..new_project("ledger", "/w", "ledger")
        };

        Projects::new(&Store::default(), &disk, &Frozen).create(&request).unwrap();

        assert_eq!(*LOG.lock().unwrap(), vec!["mkdir /w/ledger"]);
    }

    #[test]
    fn an_empty_existing_directory_is_reused() {
        let disk = Disk::with(&[("/w", false), ("/w/ledger", true)]);
        let created = Projects::new(&Store::default(), &disk, &Frozen)
            .create(&new_project("ledger", "/w", "ledger"));
        assert!(created.is_ok());
    }

    #[test]
    fn an_occupied_directory_is_refused_and_nothing_is_written() {
        let store = Store::default();
        let disk = Disk::with(&[("/w", false), ("/w/ledger", false)]);

        let result = Projects::new(&store, &disk, &Frozen).create(&new_project("ledger", "/w", "ledger"));

        assert!(matches!(result, Err(ProjectError::Invalid(_))));
        assert!(store.list().is_empty());
    }

    #[test]
    fn a_missing_parent_is_refused_before_anything_is_made() {
        let disk = Disk::with(&[]);
        let result =
            Projects::new(&Store::default(), &disk, &Frozen).create(&new_project("ledger", "/nope", "ledger"));
        assert!(matches!(result, Err(ProjectError::Missing(_))));
        assert_eq!(disk.state("/nope/ledger"), PathState::Missing);
    }

    #[test]
    fn a_taken_name_is_refused_before_the_directory_is_made() {
        let store = Store::default();
        store.insert(&record("ledger", "개인", "/elsewhere")).unwrap();
        let disk = Disk::with(&[("/w", false)]);

        let result = Projects::new(&store, &disk, &Frozen).create(&new_project("ledger", "/w", "ledger"));

        assert!(matches!(result, Err(ProjectError::Taken(_))));
        assert_eq!(disk.state("/w/ledger"), PathState::Missing);
    }

    #[test]
    fn an_invalid_name_is_refused() {
        let result = Projects::new(&Store::default(), &Disk::with(&[("/w", false)]), &Frozen)
            .create(&new_project("가계부", "/w", "ledger"));
        assert!(matches!(result, Err(ProjectError::Invalid(_))));
    }
}

mod register {
    use super::*;

    #[test]
    fn records_an_existing_directory_without_touching_it() {
        static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let store = Store::default();
        let disk = Disk {
            log: Some(&LOG),
            ..Disk::with(&[("/w/api", false)])
        };

        let saved = Projects::new(&store, &disk, &Frozen)
            .register(&registration("api", "/w/api"))
            .unwrap();

        assert!(LOG.lock().unwrap().is_empty());
        assert_eq!(saved.origin, Origin::Registered);
        assert_eq!(store.load("api").unwrap(), saved);
    }

    #[test]
    fn a_path_that_is_already_a_project_is_refused() {
        let store = Store::default();
        store.insert(&record("api", "tuk", "/w/api")).unwrap();
        let result = Projects::new(&store, &Disk::with(&[("/w/api", false)]), &Frozen)
            .register(&registration("api-2", "/w/api"));
        assert!(matches!(result, Err(ProjectError::Taken(_))));
    }

    #[test]
    fn a_missing_directory_is_refused() {
        let result = Projects::new(&Store::default(), &Disk::with(&[]), &Frozen)
            .register(&registration("api", "/w/api"));
        assert!(matches!(result, Err(ProjectError::Missing(_))));
    }
}

mod list {
    use super::*;

    #[test]
    fn orders_by_group_then_name_and_keeps_unreadable_records_as_errors() {
        let store = Store {
            unreadable: vec!["broken/project.toml을 읽지 못했습니다".into()],
            ..Store::default()
        };
        store.insert(&record("zeta", "tuk", "/w/zeta")).unwrap();
        store.insert(&record("alpha", "개인", "/w/alpha")).unwrap();
        store.insert(&record("beta", "tuk", "/w/beta")).unwrap();
        let disk = Disk::with(&[("/w/zeta", false), ("/w/alpha", false), ("/w/beta", false)]);

        let (found, errors) = Projects::new(&store, &disk, &Frozen).list();

        let names: Vec<&str> = found.iter().map(|o| o.record.name.as_str()).collect();
        assert_eq!(names, vec!["beta", "zeta", "alpha"]);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn a_project_whose_directory_is_gone_shows_a_local_warning() {
        let store = Store::default();
        store.insert(&record("gone", "tuk", "/w/gone")).unwrap();

        let (found, _) = Projects::new(&store, &Disk::with(&[]), &Frozen).list();

        assert!(found[0].scan.is_err());
        assert_eq!(found[0].stages.local, StageState::Warn);
    }
}

mod stages {
    use super::*;

    fn overview_with(scan: LocalScan) -> secrets_core::project::Overview {
        let store = Store::default();
        store.insert(&record("api", "tuk", "/w/api")).unwrap();
        let disk = Disk {
            scans: BTreeMap::from([("/w/api".to_string(), scan)]),
            ..Disk::with(&[("/w/api", false)])
        };
        Projects::new(&store, &disk, &Frozen).overview("api").unwrap()
    }

    #[test]
    fn an_origin_remote_completes_the_git_stage() {
        let overview = overview_with(LocalScan {
            git: GitState::Remote {
                branch: Some("main".into()),
                commits: 3,
                changes: 0,
                origin: "git@github.com:o/api.git".into(),
            },
            ..plain_scan()
        });
        assert_eq!(overview.stages.git, StageState::Done);
        assert_eq!(overview.stages.server, StageState::Pending);
    }

    #[test]
    fn a_local_repository_leaves_the_git_stage_pending() {
        let overview = overview_with(LocalScan {
            git: GitState::Local {
                branch: Some("main".into()),
                commits: 0,
                changes: 2,
            },
            ..plain_scan()
        });
        assert_eq!(overview.stages.git, StageState::Pending);
        assert_eq!(overview.stages.local, StageState::Done);
    }

    #[test]
    fn an_exposed_env_file_puts_a_warning_on_the_local_stage() {
        let overview = overview_with(LocalScan {
            env_files: vec![EnvFileFact {
                name: ".env.local".into(),
                variables: 4,
                tracked: false,
                ignored: Some(false),
            }],
            ..plain_scan()
        });
        assert_eq!(overview.stages.local, StageState::Warn);
    }
}

mod ignore_exposed {
    use super::*;

    #[test]
    fn adds_only_untracked_value_files_that_git_would_pick_up() {
        static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let store = Store::default();
        store.insert(&record("api", "tuk", "/w/api")).unwrap();
        let fact = |name: &str, tracked: bool, ignored: Option<bool>| EnvFileFact {
            name: name.into(),
            variables: 1,
            tracked,
            ignored,
        };
        let disk = Disk {
            log: Some(&LOG),
            scans: BTreeMap::from([(
                "/w/api".to_string(),
                LocalScan {
                    env_files: vec![
                        fact(".env.example", false, Some(false)),
                        fact(".env.local", false, Some(false)),
                        fact(".env.prod", true, Some(false)),
                        fact(".env.dev", false, Some(true)),
                    ],
                    ..plain_scan()
                },
            )]),
            ..Disk::with(&[("/w/api", false)])
        };

        let added = Projects::new(&store, &disk, &Frozen).ignore_exposed("api").unwrap();

        assert_eq!(added, vec![".env.local"]);
        assert_eq!(*LOG.lock().unwrap(), vec!["gitignore /w/api .env.local"]);
    }
}
