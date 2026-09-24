//! 계정 만들기가 중간에 죽으면 무엇이 남는가.
//!
//! 서버가 끼어드는 절차라 단계 사이에서 죽을 수 있다. 그때 **화면이 말할 수 있는
//! 자리**에만 멈추는지를 본다. 실제 서버 없이 확인해야 의미가 있으므로 가짜를 쓴다.

use std::collections::HashMap;
use std::sync::Mutex;

use secrets_core::aws::instance::{
    AccountState, HostError, InstanceAccount, InstanceGateway, InstanceVault, Plan, Readiness,
    Role, Seat,
};
use secrets_core::aws::provisioning::Provisioning;
use secrets_core::port::{Clock, ProgressSink, Silent};

struct Frozen;

impl Clock for Frozen {
    fn now(&self) -> String {
        "2026-09-23T10:00:00+09:00".into()
    }
    fn today(&self) -> String {
        "2026-09-23".into()
    }
}

#[derive(Default)]
struct Server {
    /// 계정 → 심어진 공개 키.
    accounts: Mutex<HashMap<String, String>>,
    /// 우리가 만들기 전부터 있던 계정.
    existing: Mutex<Vec<String>>,
    refuse_install: Mutex<bool>,
    /// 심기는 되는데 들어갈 수는 없는 상황.
    refuse_login: Mutex<bool>,
    refuse_remove: Mutex<bool>,
    /// 서버에 acl 이 깔려 있는가.
    has_acl: Mutex<bool>,
}

fn ready(acl: bool) -> Readiness {
    Readiness {
        sudo: true,
        acl,
        useradd: true,
        visudo: true,
        packager: Some("apt-get".into()),
    }
}

impl InstanceGateway for Server {
    fn inspect(
        &self,
        _pem: &str,
        _plan: &Plan,
        _progress: &dyn ProgressSink,
    ) -> Result<Readiness, HostError> {
        Ok(ready(*self.has_acl.lock().unwrap()))
    }

    fn prepare(
        &self,
        _pem: &str,
        _plan: &Plan,
        _progress: &dyn ProgressSink,
    ) -> Result<Readiness, HostError> {
        *self.has_acl.lock().unwrap() = true;
        Ok(ready(true))
    }

    fn install(
        &self,
        _pem: &str,
        seat: &Seat,
        _plan: &Plan,
        public_key: &str,
        _progress: &dyn ProgressSink,
    ) -> Result<bool, HostError> {
        if *self.refuse_install.lock().unwrap() {
            return Err(HostError::Remote("거절".into()));
        }
        let made = !self.existing.lock().unwrap().contains(&seat.account);
        self.accounts
            .lock()
            .unwrap()
            .insert(seat.account.clone(), public_key.to_string());
        Ok(made)
    }

    fn verify(
        &self,
        _private_key: &str,
        seat: &Seat,
        _plan: &Plan,
        _progress: &dyn ProgressSink,
    ) -> Result<(), HostError> {
        if *self.refuse_login.lock().unwrap() {
            return Err(HostError::Unreachable("연결 거부".into()));
        }
        if self.accounts.lock().unwrap().contains_key(&seat.account) {
            Ok(())
        } else {
            Err(HostError::Unreachable("그런 계정이 없습니다".into()))
        }
    }

    fn remove(
        &self,
        _pem: &str,
        seat: &Seat,
        _plan: &Plan,
        ours: bool,
        _progress: &dyn ProgressSink,
    ) -> Result<(), HostError> {
        if *self.refuse_remove.lock().unwrap() {
            return Err(HostError::Remote("거절".into()));
        }
        if ours {
            self.accounts.lock().unwrap().remove(&seat.account);
        }
        Ok(())
    }
}

#[derive(Default)]
struct Disk {
    placed: Mutex<HashMap<String, String>>,
    records: Mutex<HashMap<String, InstanceAccount>>,
    archived: Mutex<Vec<String>>,
    serial: Mutex<u32>,
}

impl InstanceVault for Disk {
    fn exists(&self, seat: &Seat) -> bool {
        self.records.lock().unwrap().contains_key(&seat.slug())
    }

    fn create(&self, seat: &Seat, _comment: &str) -> Result<(String, String, String), HostError> {
        let mut serial = self.serial.lock().unwrap();
        *serial += 1;
        let public = format!("ssh-ed25519 pub-{serial}");
        self.placed
            .lock()
            .unwrap()
            .insert(seat.slug(), public.clone());
        Ok((public.clone(), format!("SHA256:{serial}"), "ed25519".into()))
    }

    fn public_key(&self, seat: &Seat) -> Result<String, HostError> {
        self.placed
            .lock()
            .unwrap()
            .get(&seat.slug())
            .cloned()
            .ok_or_else(|| HostError::Missing(seat.slug()))
    }

    fn private_path(&self, seat: &Seat) -> String {
        format!("/vault/{}/key", seat.slug())
    }

    fn record(&self, account: &InstanceAccount) -> Result<(), HostError> {
        self.records
            .lock()
            .unwrap()
            .insert(account.slug(), account.clone());
        Ok(())
    }

    fn load(&self, seat: &Seat) -> Result<InstanceAccount, HostError> {
        self.records
            .lock()
            .unwrap()
            .get(&seat.slug())
            .cloned()
            .ok_or_else(|| HostError::Missing(seat.slug()))
    }

    fn list(&self) -> Vec<Result<InstanceAccount, String>> {
        let mut all: Vec<InstanceAccount> = self.records.lock().unwrap().values().cloned().collect();
        all.sort_by(|a, b| a.account.cmp(&b.account));
        all.into_iter().map(Ok).collect()
    }

    fn archive(&self, seat: &Seat, _reason: &str) -> Result<(), HostError> {
        self.records.lock().unwrap().remove(&seat.slug());
        self.placed.lock().unwrap().remove(&seat.slug());
        self.archived.lock().unwrap().push(seat.slug());
        Ok(())
    }

    fn discard(&self, seat: &Seat) {
        self.records.lock().unwrap().remove(&seat.slug());
        self.placed.lock().unwrap().remove(&seat.slug());
    }
}

struct Fixture {
    server: Server,
    disk: Disk,
    clock: Frozen,
}

impl Fixture {
    fn new() -> Fixture {
        let server = Server::default();
        *server.has_acl.lock().unwrap() = true;
        Fixture {
            server,
            disk: Disk::default(),
            clock: Frozen,
        }
    }

    fn work(&self) -> Provisioning<'_> {
        Provisioning::new(&self.server, &self.disk, &self.clock)
    }
}

fn seat(account: &str) -> Seat {
    Seat::new("ap-northeast-2", "gonggugyeong", "i-083b9ac0", account).unwrap()
}

fn plan(role: Role) -> Plan {
    Plan {
        role,
        workspace: "/srv".into(),
        group: "workspace".into(),
        via: "ubuntu".into(),
        address: "43.200.159.9".into(),
    }
}

#[test]
fn a_new_account_is_installed_and_then_actually_opened() {
    let f = Fixture::new();
    let at = seat("deploy");

    let made = f
        .work()
        .create("pem", &at, &plan(Role::User), "배포", "gonggugyeong-server", &Silent)
        .unwrap();

    assert_eq!(made.state, AccountState::Verified);
    assert!(made.verified_at.is_some(), "들어가 본 시각이 남아야 한다");
    assert!(made.ours, "우리가 만든 계정이다");
}

#[test]
fn a_key_that_cannot_be_installed_leaves_nothing_behind() {
    let f = Fixture::new();
    let at = seat("deploy");
    *f.server.refuse_install.lock().unwrap() = true;

    assert!(
        f.work()
            .create("pem", &at, &plan(Role::User), "", "", &Silent)
            .is_err()
    );

    // 서버에 아무것도 안 남았으니 쓸 수 없는 키를 금고에 남길 이유가 없다.
    assert!(!f.disk.exists(&at));
}

#[test]
fn an_account_that_cannot_be_entered_is_not_called_verified() {
    let f = Fixture::new();
    let at = seat("deploy");
    // 심기는 되는데 들어갈 수 없는 상황 — 오타 하나로 충분히 생긴다.
    *f.server.refuse_login.lock().unwrap() = true;

    assert!(matches!(
        f.work().create("pem", &at, &plan(Role::User), "", "", &Silent),
        Err(HostError::Unreachable(_))
    ));

    // 기록은 남는다. 조용히 사라지면 서버에 남은 계정을 아무도 모른다.
    let stuck = f.disk.load(&at).unwrap();
    assert_eq!(stuck.state, AccountState::Installed);
    assert!(stuck.verified_at.is_none());
}

#[test]
fn a_stalled_account_finishes_where_it_stopped_without_a_new_key() {
    let f = Fixture::new();
    let at = seat("deploy");
    *f.server.refuse_login.lock().unwrap() = true;
    let _ = f.work().create("pem", &at, &plan(Role::User), "", "", &Silent);
    let key = f.disk.public_key(&at).unwrap();

    *f.server.refuse_login.lock().unwrap() = false;
    let done = f.work().reinstall("pem", &at, &plan(Role::User), &Silent).unwrap();

    assert_eq!(done.state, AccountState::Verified);
    // 같은 키다. 새로 만들면 이미 심은 공개 키가 짝을 잃는다.
    assert_eq!(f.disk.public_key(&at).unwrap(), key);
}

#[test]
fn an_account_we_did_not_create_is_left_on_the_server() {
    let f = Fixture::new();
    let at = seat("ubuntu");
    f.server.existing.lock().unwrap().push("ubuntu".into());

    let made = f
        .work()
        .create("pem", &at, &plan(Role::User), "", "", &Silent)
        .unwrap();
    assert!(!made.ours, "있던 계정을 우리 것이라고 적으면 안 된다");

    f.work().remove("pem", &at, &Silent).unwrap();
    // 키와 권한은 걷었지만 계정 자체는 남는다.
    assert!(f.server.accounts.lock().unwrap().contains_key("ubuntu"));
}

#[test]
fn removing_clears_the_server_before_the_record_is_archived() {
    let f = Fixture::new();
    let at = seat("deploy");
    f.work()
        .create("pem", &at, &plan(Role::User), "", "", &Silent)
        .unwrap();

    *f.server.refuse_remove.lock().unwrap() = true;
    assert!(f.work().remove("pem", &at, &Silent).is_err());

    // 서버에서 못 걷었으면 기록도 남는다. 먼저 치우면 무엇을 어디서 지울지 잃는다.
    assert!(f.disk.exists(&at));
    assert!(f.disk.archived.lock().unwrap().is_empty());

    *f.server.refuse_remove.lock().unwrap() = false;
    f.work().remove("pem", &at, &Silent).unwrap();
    assert!(f.server.accounts.lock().unwrap().is_empty());
    assert_eq!(f.disk.archived.lock().unwrap().len(), 1);
}

#[test]
fn a_seat_refuses_names_linux_would_not_take() {
    for bad in ["", "  ", "Deploy", "1deploy", "de ploy", "deploy!", "..", "de/ploy"] {
        assert!(
            Seat::new("ap-northeast-2", "gonggugyeong", "i-08", bad).is_none(),
            "{bad}"
        );
    }
    for good in ["deploy", "ai-agent", "_svc", "web2"] {
        assert!(
            Seat::new("ap-northeast-2", "gonggugyeong", "i-08", good).is_some(),
            "{good}"
        );
    }
}

#[test]
fn an_account_already_verified_can_still_be_reinstalled() {
    let f = Fixture::new();
    let at = seat("deploy");
    f.work()
        .create("pem", &at, &plan(Role::User), "", "", &Silent)
        .unwrap();
    let key = f.disk.public_key(&at).unwrap();

    // 서버 설정이 바뀌어 다시 걸어야 하는 경우가 있다. 막아 두면 손쓸 방법이 없다.
    let again = f
        .work()
        .reinstall("pem", &at, &plan(Role::User), &Silent)
        .unwrap();

    assert_eq!(again.state, AccountState::Verified);
    assert_eq!(f.disk.public_key(&at).unwrap(), key, "같은 키여야 한다");
}

#[test]
fn an_instance_without_acl_gets_no_account_at_all() {
    let f = Fixture::new();
    let at = seat("deploy");
    *f.server.has_acl.lock().unwrap() = false;

    let refused = f
        .work()
        .create("pem", &at, &plan(Role::User), "", "", &Silent);

    // 반만 도는 계정을 만드는 것보다 안 만드는 게 낫다. 공용 자리 쓰기 공유가
    // umask 에 좌우되면 나중에 알아차리기 어렵다.
    assert!(matches!(refused, Err(HostError::NotReady(ref gaps)) if gaps.contains(&"setfacl")));
    assert!(!f.disk.exists(&at));
    assert!(f.server.accounts.lock().unwrap().is_empty());
}

#[test]
fn preparing_the_instance_makes_it_ready() {
    let f = Fixture::new();
    let at = seat("deploy");
    *f.server.has_acl.lock().unwrap() = false;

    let before = f.work().inspect("pem", &plan(Role::User), &Silent).unwrap();
    assert!(!before.ok());
    assert_eq!(before.missing(), vec!["setfacl"]);

    f.work().prepare("pem", &plan(Role::User), &Silent).unwrap();

    let after = f.work().inspect("pem", &plan(Role::User), &Silent).unwrap();
    assert!(after.ok());
    assert!(
        f.work()
            .create("pem", &at, &plan(Role::User), "", "", &Silent)
            .is_ok()
    );
}

#[test]
fn reinstalling_does_not_forget_that_we_created_the_account() {
    let f = Fixture::new();
    let at = seat("deploy");
    f.work()
        .create("pem", &at, &plan(Role::User), "", "", &Silent)
        .unwrap();

    // 다시 심으면 계정이 이미 있으니 "만들지 않았다" 가 온다. 그걸 그대로 적으면
    // 우리가 만든 계정을 나중에 지우지 못하고 서버에 남는다.
    let again = f
        .work()
        .reinstall("pem", &at, &plan(Role::User), &Silent)
        .unwrap();
    assert!(again.ours, "처음 만들었다는 사실이 지워지면 안 된다");

    f.work().remove("pem", &at, &Silent).unwrap();
    assert!(f.server.accounts.lock().unwrap().is_empty(), "계정이 지워져야 한다");
}
