//! 계정 만들기가 중간에 죽으면 무엇이 남는가.
//!
//! 서버가 끼어드는 절차라 단계 사이에서 죽을 수 있다. 그때 **화면이 말할 수 있는 자리**에만
//! 멈추는지를 본다. 실제 서버 없이 확인해야 의미가 있으므로 가짜를 쓴다.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use secrets_core::port::{Clock, ProgressSink, Silent};
use secrets_core::server::{
    Access, AccountKey, AccountKeys, AccountOrigin, AccountProvisioning, AccountState, CreatedKey,
    Install, InstallMode, Readiness, Role, Server, ServerAccount, ServerError, ServerGateway,
    ServerKind, ServerStore,
};

struct Frozen;

impl Clock for Frozen {
    fn now(&self) -> String {
        "2026-09-27T12:00:00+09:00".into()
    }
    fn today(&self) -> String {
        "2026-09-27".into()
    }
}

#[derive(Default)]
struct Memory(Mutex<BTreeMap<String, Server>>);

impl ServerStore for Memory {
    fn list(&self) -> Vec<Result<Server, String>> {
        self.0.lock().unwrap().values().cloned().map(Ok).collect()
    }
    fn load(&self, id: &str) -> Result<Server, ServerError> {
        self.0
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| ServerError::Missing(id.into()))
    }
    fn insert(&self, server: &Server) -> Result<(), ServerError> {
        self.0
            .lock()
            .unwrap()
            .insert(server.id.clone(), server.clone());
        Ok(())
    }
    fn replace(&self, server: &Server) -> Result<(), ServerError> {
        self.0
            .lock()
            .unwrap()
            .insert(server.id.clone(), server.clone());
        Ok(())
    }
    fn archive(&self, id: &str) -> Result<(), ServerError> {
        self.0.lock().unwrap().remove(id);
        Ok(())
    }
}

/// 키 저장소. 만든 키의 공개 키를 기억하고, 치우거나 보관한 키를 적어 둔다.
#[derive(Default)]
struct Keys {
    public: Mutex<HashMap<String, String>>,
    discarded: Mutex<Vec<String>>,
    archived: Mutex<Vec<String>>,
    serial: Mutex<u32>,
}

fn path_of(key: &AccountKey) -> String {
    match key {
        AccountKey::Vault { path } | AccountKey::File { path } => path.clone(),
        AccountKey::Pem { keypair } => format!("pem/{keypair}"),
        AccountKey::Agent => "agent".into(),
    }
}

impl AccountKeys for Keys {
    fn create(&self, server: &str, login: &str, _: &str) -> Result<CreatedKey, ServerError> {
        let mut serial = self.serial.lock().unwrap();
        *serial += 1;
        let path = format!("keys/server/{server}/{login}/key");
        let public = format!("ssh-ed25519 pub-{serial}");
        self.public
            .lock()
            .unwrap()
            .insert(path.clone(), public.clone());
        Ok(CreatedKey {
            key: AccountKey::Vault { path },
            public_key: public,
            fingerprint: format!("SHA256:{serial}"),
        })
    }
    fn import(&self, _: &str, _: &str, _: &str) -> Result<AccountKey, ServerError> {
        unreachable!()
    }
    fn public_key(&self, _: &Server, key: &AccountKey) -> Result<String, ServerError> {
        self.public
            .lock()
            .unwrap()
            .get(&path_of(key))
            .cloned()
            .ok_or_else(|| ServerError::Missing(path_of(key)))
    }
    fn private_path(&self, _: &Server, key: &AccountKey) -> Result<Option<String>, ServerError> {
        Ok(match key {
            AccountKey::Agent => None,
            other => Some(format!("/vault/{}", path_of(other))),
        })
    }
    fn discard(&self, key: &AccountKey) {
        self.public.lock().unwrap().remove(&path_of(key));
        self.discarded.lock().unwrap().push(path_of(key));
    }
    fn archive(&self, _: &str, key: &AccountKey) -> Result<(), ServerError> {
        self.archived.lock().unwrap().push(path_of(key));
        Ok(())
    }
}

/// 서버. 계정마다 심어진 공개 키, 그리고 누가 어떤 키로 들어왔는지.
#[derive(Default)]
struct Machine {
    accounts: Mutex<HashMap<String, String>>,
    existing: Mutex<Vec<String>>,
    refuse_install: Mutex<bool>,
    refuse_login: Mutex<bool>,
    refuse_remove: Mutex<bool>,
    no_acl: Mutex<bool>,
    entered_as: Mutex<Vec<String>>,
}

impl Machine {
    fn ready(&self) -> Readiness {
        Readiness {
            sudo: true,
            acl: !*self.no_acl.lock().unwrap(),
            useradd: true,
            visudo: true,
            packager: Some("apt-get".into()),
        }
    }
}

impl ServerGateway for Machine {
    fn inspect(&self, admin: &Access, _: &dyn ProgressSink) -> Result<Readiness, ServerError> {
        self.entered_as
            .lock()
            .unwrap()
            .push(format!("{}@{}:{}", admin.login, admin.address, admin.port));
        Ok(self.ready())
    }
    fn prepare(&self, _: &Access, _: &dyn ProgressSink) -> Result<Readiness, ServerError> {
        *self.no_acl.lock().unwrap() = false;
        Ok(self.ready())
    }
    fn install(
        &self,
        _: &Access,
        install: &Install,
        _: &dyn ProgressSink,
    ) -> Result<bool, ServerError> {
        if *self.refuse_install.lock().unwrap() {
            return Err(ServerError::Remote("거절".into()));
        }
        let made = !self.existing.lock().unwrap().contains(&install.login)
            && !self.accounts.lock().unwrap().contains_key(&install.login);
        if !made && install.mode == InstallMode::New {
            return Err(ServerError::Taken(install.login.clone()));
        }
        self.accounts
            .lock()
            .unwrap()
            .insert(install.login.clone(), install.public_key.clone());
        Ok(made)
    }
    fn verify(&self, access: &Access, _: bool, _: &dyn ProgressSink) -> Result<(), ServerError> {
        if *self.refuse_login.lock().unwrap() {
            return Err(ServerError::Unreachable("연결 거부".into()));
        }
        if self.accounts.lock().unwrap().contains_key(&access.login) {
            Ok(())
        } else {
            Err(ServerError::Unreachable("그런 계정이 없습니다".into()))
        }
    }
    fn remove(
        &self,
        _: &Access,
        login: &str,
        _: &str,
        delete_account: bool,
        _: &dyn ProgressSink,
    ) -> Result<(), ServerError> {
        if *self.refuse_remove.lock().unwrap() {
            return Err(ServerError::Remote("거절".into()));
        }
        if delete_account {
            self.accounts.lock().unwrap().remove(login);
        }
        Ok(())
    }
}

struct Fixture {
    store: Memory,
    keys: Keys,
    machine: Machine,
}

impl Fixture {
    /// 관리 접속이 pem 의 ubuntu 인 EC2 서버 하나.
    fn new() -> Fixture {
        let f = Fixture {
            store: Memory::default(),
            keys: Keys::default(),
            machine: Machine::default(),
        };
        f.machine
            .accounts
            .lock()
            .unwrap()
            .insert("ubuntu".into(), "pem".into());
        f.store
            .insert(&Server {
                id: "gonggugyeong-server".into(),
                name: "gonggugyeong-server".into(),
                group: String::new(),
                address: "43.200.159.9".into(),
                port: 22,
                kind: ServerKind::Ec2,
                admin: Some("ubuntu".into()),
                workspace: "/srv".into(),
                workspace_group: "workspace".into(),
                note: String::new(),
                registered_at: "t".into(),
                aws: None,
                accounts: vec![ServerAccount {
                    login: "ubuntu".into(),
                    role: Role::Admin,
                    purpose: String::new(),
                    key: AccountKey::Pem {
                        keypair: "gonggugyeong".into(),
                    },
                    origin: AccountOrigin::Registered,
                    state: AccountState::Unverified,
                    verified_at: None,
                    fingerprint: String::new(),
                }],
            })
            .unwrap();
        f
    }

    fn work(&self) -> AccountProvisioning<'_> {
        AccountProvisioning::new(&self.store, &self.keys, &self.machine, &Frozen)
    }

    fn server(&self) -> Server {
        self.store.load("gonggugyeong-server").unwrap()
    }

    fn create(&self, login: &str) -> Result<ServerAccount, ServerError> {
        self.work()
            .create("gonggugyeong-server", login, Role::User, "배포", &Silent)
    }
}

#[test]
fn a_new_account_is_made_through_the_admin_access_and_then_actually_opened() {
    let f = Fixture::new();
    let made = f.create("deploy").unwrap();

    assert_eq!(made.state, AccountState::Verified);
    assert!(made.verified_at.is_some(), "들어가 본 시각이 남아야 한다");
    assert_eq!(made.origin, AccountOrigin::Created);
    assert_eq!(
        f.machine.entered_as.lock().unwrap()[0],
        "ubuntu@43.200.159.9:22",
        "관리 접속으로 들어가 만든다"
    );
    assert_eq!(f.server().account("deploy").unwrap(), &made);
}

#[test]
fn without_an_admin_access_nothing_is_made() {
    let f = Fixture::new();
    let mut server = f.server();
    server.admin = None;
    f.store.replace(&server).unwrap();

    assert!(matches!(f.create("deploy"), Err(ServerError::Invalid(_))));
    assert!(f.keys.public.lock().unwrap().is_empty());
}

#[test]
fn a_server_that_is_not_ready_gets_no_key() {
    let f = Fixture::new();
    *f.machine.no_acl.lock().unwrap() = true;

    assert!(
        matches!(f.create("deploy"), Err(ServerError::NotReady(missing)) if missing == vec!["setfacl"])
    );
    assert!(f.keys.public.lock().unwrap().is_empty());
    assert!(f.server().account("deploy").is_none());
}

#[test]
fn a_key_that_cannot_be_installed_leaves_nothing_behind() {
    let f = Fixture::new();
    *f.machine.refuse_install.lock().unwrap() = true;

    assert!(f.create("deploy").is_err());

    // 서버에 아무것도 안 남았으니 쓸 수 없는 키와 기록을 남길 이유가 없다.
    assert!(f.server().account("deploy").is_none());
    assert_eq!(
        *f.keys.discarded.lock().unwrap(),
        vec!["keys/server/gonggugyeong-server/deploy/key"]
    );
}

#[test]
fn an_account_that_cannot_be_entered_is_not_called_verified() {
    let f = Fixture::new();
    *f.machine.refuse_login.lock().unwrap() = true;

    assert!(matches!(
        f.create("deploy"),
        Err(ServerError::Unreachable(_))
    ));

    // 기록은 남는다. 조용히 사라지면 서버에 남은 계정을 아무도 모른다.
    let stuck = f.server().account("deploy").unwrap().clone();
    assert_eq!(stuck.state, AccountState::Installed);
    assert!(stuck.verified_at.is_none());
}

#[test]
fn a_stalled_account_finishes_where_it_stopped_with_the_same_key() {
    let f = Fixture::new();
    *f.machine.refuse_login.lock().unwrap() = true;
    let _ = f.create("deploy");
    let key = f.machine.accounts.lock().unwrap()["deploy"].clone();

    *f.machine.refuse_login.lock().unwrap() = false;
    let done = f
        .work()
        .reinstall("gonggugyeong-server", "deploy", &Silent)
        .unwrap();

    assert_eq!(done.state, AccountState::Verified);
    assert_eq!(
        done.origin,
        AccountOrigin::Created,
        "처음에 만든 사실은 다시 심어도 남는다"
    );
    // 같은 키다. 새로 만들면 이미 심은 공개 키가 짝을 잃는다.
    assert_eq!(f.machine.accounts.lock().unwrap()["deploy"], key);
}

#[test]
fn another_installations_account_name_is_refused_without_replacing_its_key() {
    let f = Fixture::new();
    f.machine.existing.lock().unwrap().push("app".into());
    f.machine
        .accounts
        .lock()
        .unwrap()
        .insert("app".into(), "another-persons-key".into());

    assert!(matches!(f.create("app"), Err(ServerError::Taken(_))));
    assert_eq!(
        f.machine.accounts.lock().unwrap()["app"],
        "another-persons-key"
    );
    assert!(f.server().account("app").is_none());
    assert_eq!(f.keys.discarded.lock().unwrap().len(), 1);

    let made = f.create("deploy-garden").unwrap();
    assert_eq!(made.login, "deploy-garden");
    assert_eq!(made.origin, AccountOrigin::Created);
}

#[test]
fn removing_clears_the_server_before_the_record_and_key_go() {
    let f = Fixture::new();
    f.create("deploy").unwrap();

    *f.machine.refuse_remove.lock().unwrap() = true;
    assert!(
        f.work()
            .remove("gonggugyeong-server", "deploy", &[], &Silent)
            .is_err()
    );
    assert!(f.server().account("deploy").is_some());
    assert!(f.keys.archived.lock().unwrap().is_empty());

    *f.machine.refuse_remove.lock().unwrap() = false;
    f.work()
        .remove("gonggugyeong-server", "deploy", &[], &Silent)
        .unwrap();
    assert!(f.server().account("deploy").is_none());
    assert_eq!(
        *f.keys.archived.lock().unwrap(),
        vec!["keys/server/gonggugyeong-server/deploy/key"]
    );
}

#[test]
fn an_account_an_environment_uses_is_not_removed() {
    let f = Fixture::new();
    f.create("deploy").unwrap();
    assert!(
        f.work()
            .remove(
                "gonggugyeong-server",
                "deploy",
                &["api/prod".into()],
                &Silent
            )
            .is_err()
    );
    assert!(f.machine.accounts.lock().unwrap().contains_key("deploy"));
}

#[test]
fn a_recorded_only_account_is_neither_reinstalled_nor_removed() {
    let f = Fixture::new();
    assert!(
        f.work()
            .reinstall("gonggugyeong-server", "ubuntu", &Silent)
            .is_err()
    );
    assert!(
        f.work()
            .remove("gonggugyeong-server", "ubuntu", &[], &Silent)
            .is_err()
    );
}

mod check {
    use super::*;

    #[test]
    fn entering_marks_the_account_verified() {
        let f = Fixture::new();
        let checked = f
            .work()
            .check("gonggugyeong-server", "ubuntu", &Silent)
            .unwrap();
        assert_eq!(checked.state, AccountState::Verified);
        assert_eq!(
            checked.verified_at.as_deref(),
            Some("2026-09-27T12:00:00+09:00")
        );
    }

    #[test]
    fn a_recorded_account_that_cannot_be_entered_goes_back_to_unverified() {
        let f = Fixture::new();
        f.work()
            .check("gonggugyeong-server", "ubuntu", &Silent)
            .unwrap();
        *f.machine.refuse_login.lock().unwrap() = true;

        assert!(
            f.work()
                .check("gonggugyeong-server", "ubuntu", &Silent)
                .is_err()
        );
        assert_eq!(
            f.server().account("ubuntu").unwrap().state,
            AccountState::Unverified
        );
    }
}
