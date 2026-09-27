//! 환경 변수 동기화 — 무엇을 고를 수 있고, 무엇을 비교하고, 올린 뒤 무엇을 확인하는가.

use std::collections::BTreeMap;
use std::sync::Mutex;

use secrets_core::port::{ProgressSink, Silent};
use secrets_core::project::{
    EnvDigest, EnvFileFact, EnvState, EnvSync, Environment, GitState, LocalEnvFiles, LocalScan,
    Origin, PathState, ProjectError, ProjectRecord, ProjectStore, RepoTracking, ServerEnv,
    ServerEnvFiles, ServerSeat, ServerSeats, Workspace,
};

struct Store(Mutex<ProjectRecord>);

impl Store {
    fn new(env_file: Option<&str>) -> Store {
        Store(Mutex::new(ProjectRecord {
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
                env_file: env_file.map(str::to_string),
                server_env_file: ".env".into(),
            }],
        }))
    }
    fn env_file(&self) -> Option<String> {
        self.0.lock().unwrap().environments[0].env_file.clone()
    }
    fn server_env_file(&self) -> String {
        self.0.lock().unwrap().environments[0]
            .server_env_file
            .clone()
    }
}

impl ProjectStore for Store {
    fn list(&self) -> Vec<Result<ProjectRecord, String>> {
        vec![Ok(self.0.lock().unwrap().clone())]
    }
    fn load(&self, name: &str) -> Result<ProjectRecord, ProjectError> {
        let record = self.0.lock().unwrap().clone();
        if name == record.name {
            Ok(record)
        } else {
            Err(ProjectError::Missing(name.into()))
        }
    }
    fn insert(&self, _: &ProjectRecord) -> Result<(), ProjectError> {
        unreachable!()
    }
    fn replace(&self, record: &ProjectRecord) -> Result<(), ProjectError> {
        *self.0.lock().unwrap() = record.clone();
        Ok(())
    }
}

struct Disk;

fn fact(name: &str) -> EnvFileFact {
    EnvFileFact {
        name: name.into(),
        variables: 2,
        tracked: false,
        ignored: Some(true),
    }
}

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
            git: GitState::Absent,
            runtimes: Vec::new(),
            env_files: vec![fact(".env"), fact(".env.prod"), fact(".env.example")],
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
            key: Some("/vault/i-1/app/key".into()),
        }]
    }
}

/// 내용이 곧 해시인 가짜 — `A=1\nB=2` 는 파일 해시 "A=1\nB=2", 변수 A → "1".
fn digest_of(text: &str) -> EnvDigest {
    EnvDigest {
        file: text.into(),
        keys: text
            .lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<BTreeMap<_, _>>(),
    }
}

struct Local(&'static str);

impl LocalEnvFiles for Local {
    fn salt(&self) -> String {
        "salt".into()
    }
    fn digest(&self, path: &str, file: &str, _: &str) -> Result<EnvDigest, ProjectError> {
        assert_eq!((path, file), ("/w/api", ".env.prod"));
        Ok(digest_of(self.0))
    }
    fn read(&self, _: &str, _: &str) -> Result<Vec<u8>, ProjectError> {
        Ok(self.0.as_bytes().to_vec())
    }
}

struct Server {
    file: Mutex<Option<String>>,
    has_directory: bool,
    writes: Mutex<Vec<String>>,
    /// 읽으려 한 서버 파일 이름.
    names: Mutex<Vec<String>>,
    /// 쓰기를 받고도 파일을 바꾸지 않는다.
    lossy: bool,
}

impl Server {
    fn with(file: Option<&str>) -> Server {
        Server {
            file: Mutex::new(file.map(str::to_string)),
            has_directory: true,
            writes: Mutex::new(Vec::new()),
            names: Mutex::new(Vec::new()),
            lossy: false,
        }
    }
}

impl ServerEnvFiles for Server {
    fn digest(
        &self,
        _: &ServerSeat,
        dir: &str,
        file: &str,
        _: &str,
        _: &dyn ProgressSink,
    ) -> Result<ServerEnv, ProjectError> {
        assert_eq!(dir, "/srv/api");
        self.names.lock().unwrap().push(file.to_string());
        if !self.has_directory {
            return Ok(ServerEnv::NoDirectory);
        }
        Ok(match self.file.lock().unwrap().as_deref() {
            None => ServerEnv::Missing {
                tracking: RepoTracking::Ignored,
            },
            Some(text) => ServerEnv::Present {
                digest: digest_of(text),
                mode: Some("600".into()),
                owner: Some("app".into()),
                tracking: RepoTracking::Ignored,
            },
        })
    }
    fn write(
        &self,
        _: &ServerSeat,
        dir: &str,
        file: &str,
        contents: &[u8],
        _: &dyn ProgressSink,
    ) -> Result<(), ProjectError> {
        let text = String::from_utf8(contents.to_vec()).unwrap();
        self.writes
            .lock()
            .unwrap()
            .push(format!("{dir}/{file} {text}"));
        if !self.lossy {
            *self.file.lock().unwrap() = Some(text);
        }
        Ok(())
    }
}

mod choose {
    use super::*;

    #[test]
    fn records_a_root_env_file_and_can_clear_it() {
        let store = Store::new(None);
        let (local, server) = (Local(""), Server::with(None));
        let sync = EnvSync::new(&store, &Disk, &Seats, &local, &server);

        sync.choose("api", "prod", Some(".env.prod"), ".env.local")
            .unwrap();
        assert_eq!(store.env_file().as_deref(), Some(".env.prod"));
        assert_eq!(store.server_env_file(), ".env.local");

        sync.choose("api", "prod", None, ".env").unwrap();
        assert_eq!(store.env_file(), None);
    }

    #[test]
    fn refuses_an_example_a_missing_file_and_an_unknown_environment() {
        let store = Store::new(None);
        let (local, server) = (Local(""), Server::with(None));
        let sync = EnvSync::new(&store, &Disk, &Seats, &local, &server);

        assert!(
            sync.choose("api", "prod", Some(".env.example"), ".env")
                .is_err()
        );
        assert!(
            sync.choose("api", "prod", Some(".env.dev"), ".env")
                .is_err()
        );
        assert!(
            sync.choose("api", "dev", Some(".env.prod"), ".env")
                .is_err()
        );
        for bad in ["", "../.env", "config/.env", "..", ".env x"] {
            assert!(
                sync.choose("api", "prod", Some(".env.prod"), bad).is_err(),
                "{bad}"
            );
        }
        assert_eq!(store.env_file(), None);
    }
}

mod compare {
    use super::*;

    #[test]
    fn needs_a_chosen_file_first() {
        let store = Store::new(None);
        let (local, server) = (Local("A=1"), Server::with(Some("A=1")));
        let sync = EnvSync::new(&store, &Disk, &Seats, &local, &server);
        assert!(sync.compare("api", "prod", &Silent).is_err());
    }

    #[test]
    fn reports_the_server_file_and_what_differs_by_name() {
        let store = Store::new(Some(".env.prod"));
        let (local, server) = (Local("A=1\nB=2"), Server::with(Some("B=x\nC=3")));
        let sync = EnvSync::new(&store, &Disk, &Seats, &local, &server);

        let found = sync.compare("api", "prod", &Silent).unwrap();

        assert_eq!(found.server_file, "/srv/api/.env");
        assert_eq!(
            found.state,
            EnvState::Differ {
                local_only: vec!["A".into()],
                server_only: vec!["C".into()],
                changed: vec!["B".into()],
            }
        );
        assert!(
            server.writes.lock().unwrap().is_empty(),
            "비교는 서버에 쓰지 않는다"
        );
    }

    #[test]
    fn reads_the_server_file_under_the_chosen_name() {
        let store = Store::new(None);
        let (local, server) = (Local("A=1"), Server::with(Some("A=1")));
        let sync = EnvSync::new(&store, &Disk, &Seats, &local, &server);
        sync.choose("api", "prod", Some(".env.prod"), ".env.local")
            .unwrap();

        let found = sync.compare("api", "prod", &Silent).unwrap();

        assert_eq!(found.server_file, "/srv/api/.env.local");
        assert_eq!(
            *server.names.lock().unwrap(),
            vec![".env.local".to_string()]
        );
    }

    #[test]
    fn tells_a_missing_file_from_a_missing_directory() {
        let store = Store::new(Some(".env.prod"));
        let local = Local("A=1");
        let missing = Server::with(None);
        let sync = EnvSync::new(&store, &Disk, &Seats, &local, &missing);
        assert_eq!(
            sync.compare("api", "prod", &Silent).unwrap().state,
            EnvState::ServerMissing
        );

        let no_dir = Server {
            has_directory: false,
            ..Server::with(None)
        };
        let sync = EnvSync::new(&store, &Disk, &Seats, &local, &no_dir);
        assert_eq!(
            sync.compare("api", "prod", &Silent).unwrap().state,
            EnvState::NoDirectory
        );
    }
}

mod push {
    use super::*;

    #[test]
    fn writes_the_local_file_to_the_server_root_and_confirms_it_matches() {
        let store = Store::new(Some(".env.prod"));
        let (local, server) = (Local("A=1\nB=2"), Server::with(Some("A=0")));
        let sync = EnvSync::new(&store, &Disk, &Seats, &local, &server);

        let after = sync.push("api", "prod", &Silent).unwrap();

        assert_eq!(after.state, EnvState::Same);
        assert_eq!(
            *server.writes.lock().unwrap(),
            vec!["/srv/api/.env A=1\nB=2".to_string()]
        );
    }

    #[test]
    fn fails_when_the_server_does_not_match_after_writing() {
        let store = Store::new(Some(".env.prod"));
        let local = Local("A=1");
        let server = Server {
            lossy: true,
            ..Server::with(Some("A=0"))
        };
        let sync = EnvSync::new(&store, &Disk, &Seats, &local, &server);
        assert!(sync.push("api", "prod", &Silent).is_err());
    }
}
