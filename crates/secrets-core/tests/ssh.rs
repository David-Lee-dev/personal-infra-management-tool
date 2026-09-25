//! SSH 접속 설정 — 기록한 별칭으로 그룹별 conf 를 만들고, 계정이 바뀌면 따라가는가.

use std::collections::BTreeMap;
use std::sync::Mutex;

use secrets_core::project::{ServerSeat, ServerSeats};
use secrets_core::ssh::{SshConfig, SshError, SshFiles, SshHost, SshStore, UserConfig, alias_for};

#[derive(Default)]
struct Store(Mutex<Vec<SshHost>>);

impl SshStore for Store {
    fn load(&self) -> Result<Vec<SshHost>, SshError> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn save(&self, hosts: &[SshHost]) -> Result<(), SshError> {
        *self.0.lock().unwrap() = hosts.to_vec();
        Ok(())
    }
}

#[derive(Default)]
struct Files {
    written: Mutex<BTreeMap<String, String>>,
    user: UserConfig,
    includes: Mutex<u32>,
}

impl SshFiles for Files {
    fn groups(&self) -> Vec<String> {
        self.written.lock().unwrap().keys().cloned().collect()
    }
    fn write_group(&self, group: &str, text: &str) -> Result<(), SshError> {
        self.written
            .lock()
            .unwrap()
            .insert(group.into(), text.into());
        Ok(())
    }
    fn remove_group(&self, group: &str) -> Result<(), SshError> {
        self.written.lock().unwrap().remove(group);
        Ok(())
    }
    fn user_config(&self) -> UserConfig {
        self.user.clone()
    }
    fn add_include(&self) -> Result<Option<String>, SshError> {
        *self.includes.lock().unwrap() += 1;
        Ok(Some("/archive/config".into()))
    }
}

struct Seats(Mutex<Vec<ServerSeat>>);

fn seat(instance: &str, login: &str) -> ServerSeat {
    ServerSeat {
        aws_account: "1".into(),
        machine: "ec2".into(),
        region: "r".into(),
        keypair: "k".into(),
        instance: instance.into(),
        instance_name: format!("{instance}-name"),
        address: format!("{instance}.example"),
        login: login.into(),
        admin: false,
        verified: true,
        key_path: format!("/vault/{instance}/{login}/key"),
    }
}

impl ServerSeats for Seats {
    fn seats(&self) -> Vec<ServerSeat> {
        self.0.lock().unwrap().clone()
    }
}

fn host(alias: &str, group: &str, instance: &str, login: &str) -> SshHost {
    SshHost {
        alias: alias.into(),
        group: group.into(),
        instance: instance.into(),
        login: login.into(),
    }
}

struct World {
    store: Store,
    files: Files,
    seats: Seats,
}

impl World {
    fn new() -> World {
        World {
            store: Store::default(),
            files: Files::default(),
            seats: Seats(Mutex::new(vec![
                seat("i-web", "admin"),
                seat("i-web", "app"),
                seat("i-db", "ops"),
            ])),
        }
    }
    fn config(&self) -> SshConfig<'_> {
        SshConfig::new(&self.store, &self.files, &self.seats)
    }
    fn file(&self, group: &str) -> String {
        self.files
            .written
            .lock()
            .unwrap()
            .get(group)
            .cloned()
            .unwrap_or_default()
    }
}

mod add {
    use super::*;

    #[test]
    fn writes_one_conf_per_group_with_address_user_and_key_from_the_vault() {
        let world = World::new();
        world
            .config()
            .add(&host("web", "tuk", "i-web", "admin"))
            .unwrap();
        world
            .config()
            .add(&host("web-app", "tuk", "i-web", "app"))
            .unwrap();
        world
            .config()
            .add(&host("gong", "공구경", "i-db", "ops"))
            .unwrap();

        let tuk = world.file("tuk");
        assert!(tuk.contains("Host web\n    HostName i-web.example\n    User admin\n    IdentityFile /vault/i-web/admin/key\n    IdentitiesOnly yes\n"));
        assert!(tuk.contains("Host web-app\n"));
        assert!(!tuk.contains("Host gong"));
        assert!(world.file("공구경").contains("Host gong\n"));
    }

    #[test]
    fn refuses_a_duplicate_alias_a_bad_alias_and_an_unknown_account() {
        let world = World::new();
        world
            .config()
            .add(&host("web", "tuk", "i-web", "admin"))
            .unwrap();

        assert!(
            world
                .config()
                .add(&host("web", "tuk", "i-web", "app"))
                .is_err()
        );
        assert!(
            world
                .config()
                .add(&host("web *", "tuk", "i-web", "app"))
                .is_err()
        );
        assert!(
            world
                .config()
                .add(&host("x", "tuk", "i-web", "nobody"))
                .is_err()
        );
        assert!(
            world
                .config()
                .add(&host("x", "a/b", "i-web", "app"))
                .is_err()
        );
        assert_eq!(world.store.load().unwrap().len(), 1);
    }
}

mod regenerate {
    use super::*;

    #[test]
    fn follows_account_changes_and_comments_out_what_is_gone() {
        let world = World::new();
        world
            .config()
            .add(&host("web", "tuk", "i-web", "admin"))
            .unwrap();

        // 계정이 보관소로 옮겨졌다.
        world.seats.0.lock().unwrap().retain(|s| s.login != "admin");
        world.config().regenerate().unwrap();

        let tuk = world.file("tuk");
        assert!(!tuk.contains("Host web\n"));
        assert!(tuk.contains(
            "# web: 서버 계정 i-web/admin을(를) 시크릿 저장소에서 찾을 수 없어 뺐습니다."
        ));
    }

    #[test]
    fn a_group_left_without_aliases_loses_its_file() {
        let world = World::new();
        world
            .config()
            .add(&host("gong", "공구경", "i-db", "ops"))
            .unwrap();
        world.config().remove("gong").unwrap();
        assert!(world.files.groups().is_empty());
    }
}

mod overview {
    use super::*;

    #[test]
    fn reports_aliases_also_written_by_hand() {
        let world = World {
            files: Files {
                user: UserConfig {
                    includes_ours: true,
                    aliases: vec!["web".into(), "github.com".into()],
                },
                ..Files::default()
            },
            ..World::new()
        };
        world
            .config()
            .add(&host("web", "tuk", "i-web", "admin"))
            .unwrap();
        world
            .config()
            .add(&host("db", "tuk", "i-db", "ops"))
            .unwrap();

        let overview = world.config().overview().unwrap();

        assert_eq!(overview.duplicates, vec!["web"]);
        assert!(overview.hosts.iter().all(|h| h.seat.is_some()));
    }
}

mod add_include {
    use super::*;

    #[test]
    fn is_not_added_twice() {
        let world = World {
            files: Files {
                user: UserConfig {
                    includes_ours: true,
                    aliases: vec![],
                },
                ..Files::default()
            },
            ..World::new()
        };
        assert_eq!(world.config().add_include().unwrap(), None);
        assert_eq!(*world.files.includes.lock().unwrap(), 0);
    }
}

mod alias_for {
    use super::*;

    #[test]
    fn joins_the_instance_name_and_the_account() {
        assert_eq!(
            alias_for("gonggugyeong-server", "i-083b", "admin"),
            "gonggugyeong-server-admin"
        );
    }

    #[test]
    fn falls_back_to_the_instance_id_and_cleans_unusable_characters() {
        assert_eq!(alias_for("", "i-083b", "deploy"), "i-083b-deploy");
        assert_eq!(
            alias_for("tuk api server", "i-1", "monitor"),
            "tuk-api-server-monitor"
        );
        assert_eq!(alias_for("공구경 서버", "i-1", "admin"), "i-1-admin");
    }
}
