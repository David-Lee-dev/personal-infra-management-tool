//! 키 절차가 중간에 죽어도 무엇이 남는가.
//!
//! 원격이 끼어드는 절차라 단계 사이에서 죽을 수 있다. 그때 **아무것도 끊기지 않는
//! 자리**에만 멈추는지를 본다. 실제 GitHub 없이 확인해야 의미가 있으므로 가짜를 쓴다.

use std::collections::HashMap;
use std::sync::Mutex;

use secrets_core::credential::secret::Secret;
use secrets_core::key::{
    DeployKey, KeyError, KeyGateway, KeyRef, KeyState, KeyVault, Keyring, Material, RemoteKey,
    RepoRef,
};
use secrets_core::port::{Clock, ProgressSink, Silent};

struct Frozen;

impl Clock for Frozen {
    fn now(&self) -> String {
        "2026-09-23T00:00:00+00:00".into()
    }
    fn today(&self) -> String {
        "2026-09-23".into()
    }
}

#[derive(Default)]
struct Github {
    /// id → 지문. 지금 GitHub 에 등록되어 있는 것.
    registered: Mutex<HashMap<String, String>>,
    next: Mutex<u32>,
    /// 다음 등록을 거절한다.
    refuse_register: Mutex<bool>,
    /// 다음 삭제를 거절한다 — 재발급이 중간에 멈추는 상황.
    refuse_unregister: Mutex<bool>,
}

impl Github {
    fn live(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.registered.lock().unwrap().keys().cloned().collect();
        ids.sort();
        ids
    }
}

impl KeyGateway for Github {
    fn register(
        &self,
        _account: &str,
        _at: &KeyRef,
        public_key: &str,
        _write: bool,
        _progress: &dyn ProgressSink,
    ) -> Result<String, KeyError> {
        if *self.refuse_register.lock().unwrap() {
            return Err(KeyError::Remote("거절".into()));
        }
        let mut next = self.next.lock().unwrap();
        *next += 1;
        let id = format!("remote-{next}");
        self.registered
            .lock()
            .unwrap()
            .insert(id.clone(), public_key.to_string());
        Ok(id)
    }

    fn unregister(
        &self,
        _account: &str,
        _repo: &RepoRef,
        remote_id: &str,
        _progress: &dyn ProgressSink,
    ) -> Result<(), KeyError> {
        if *self.refuse_unregister.lock().unwrap() {
            return Err(KeyError::Remote("거절".into()));
        }
        self.registered.lock().unwrap().remove(remote_id);
        Ok(())
    }

    fn deploy_keys(
        &self,
        _account: &str,
        _repo: &RepoRef,
        _progress: &dyn ProgressSink,
    ) -> Result<Vec<RemoteKey>, KeyError> {
        Ok(Vec::new())
    }

    fn account_keys(
        &self,
        _account: &str,
        _progress: &dyn ProgressSink,
    ) -> Result<Vec<RemoteKey>, KeyError> {
        Ok(Vec::new())
    }
}

#[derive(Default)]
struct Disk {
    placed: Mutex<HashMap<String, String>>,
    staged: Mutex<HashMap<String, String>>,
    records: Mutex<HashMap<String, DeployKey>>,
    archived: Mutex<Vec<String>>,
    serial: Mutex<u32>,
}

impl Disk {
    fn record_of(&self, at: &KeyRef) -> DeployKey {
        self.records.lock().unwrap().get(&at.slug()).unwrap().clone()
    }
}

impl KeyVault for Disk {
    fn exists(&self, at: &KeyRef) -> bool {
        self.placed.lock().unwrap().contains_key(&at.slug())
    }

    fn stage(&self, at: &KeyRef, _comment: &str) -> Result<Material, KeyError> {
        let mut serial = self.serial.lock().unwrap();
        *serial += 1;
        let material = format!("pub-{serial}");
        self.staged
            .lock()
            .unwrap()
            .insert(at.slug(), material.clone());
        Ok(Material {
            public_key: material.clone(),
            fingerprint: format!("SHA256:{material}"),
            algorithm: "ed25519".into(),
        })
    }

    fn place(&self, at: &KeyRef) -> Result<(), KeyError> {
        let staged = self
            .staged
            .lock()
            .unwrap()
            .remove(&at.slug())
            .ok_or_else(|| KeyError::Missing(at.slug()))?;
        self.placed.lock().unwrap().insert(at.slug(), staged);
        Ok(())
    }

    fn discard_staged(&self, at: &KeyRef) {
        self.staged.lock().unwrap().remove(&at.slug());
    }

    fn record(&self, key: &DeployKey) -> Result<(), KeyError> {
        let at = key.at().unwrap();
        self.records.lock().unwrap().insert(at.slug(), key.clone());
        Ok(())
    }

    fn load(&self, at: &KeyRef) -> Result<DeployKey, KeyError> {
        self.records
            .lock()
            .unwrap()
            .get(&at.slug())
            .cloned()
            .ok_or_else(|| KeyError::Missing(at.slug()))
    }

    fn list(&self) -> Vec<Result<DeployKey, String>> {
        let mut all: Vec<DeployKey> = self.records.lock().unwrap().values().cloned().collect();
        all.sort_by(|a, b| a.purpose.cmp(&b.purpose));
        all.into_iter().map(Ok).collect()
    }

    fn public_key(&self, at: &KeyRef) -> Result<String, KeyError> {
        self.placed
            .lock()
            .unwrap()
            .get(&at.slug())
            .cloned()
            .ok_or_else(|| KeyError::Missing(at.slug()))
    }

    fn staged_public_key(&self, at: &KeyRef) -> Result<String, KeyError> {
        self.staged
            .lock()
            .unwrap()
            .get(&at.slug())
            .cloned()
            .ok_or_else(|| KeyError::Missing(at.slug()))
    }

    fn private_key(&self, at: &KeyRef) -> Result<Secret, KeyError> {
        Ok(Secret::new(self.public_key(at)?))
    }

    fn move_to(&self, from: &KeyRef, to: &KeyRef) -> Result<(), KeyError> {
        let material = self
            .placed
            .lock()
            .unwrap()
            .remove(&from.slug())
            .ok_or_else(|| KeyError::Missing(from.slug()))?;
        self.placed.lock().unwrap().insert(to.slug(), material);
        let record = self.records.lock().unwrap().remove(&from.slug());
        if let Some(record) = record {
            self.records.lock().unwrap().insert(to.slug(), record);
        }
        Ok(())
    }

    fn archive(&self, at: &KeyRef, _reason: &str) -> Result<(), KeyError> {
        self.placed.lock().unwrap().remove(&at.slug());
        self.records.lock().unwrap().remove(&at.slug());
        self.archived.lock().unwrap().push(at.slug());
        Ok(())
    }
}

fn place() -> KeyRef {
    KeyRef::new(RepoRef::parse("david-lee-dev/nemo-play").unwrap(), "ci-deploy").unwrap()
}

struct Fixture {
    github: Github,
    disk: Disk,
    clock: Frozen,
}

impl Fixture {
    fn new() -> Fixture {
        Fixture {
            github: Github::default(),
            disk: Disk::default(),
            clock: Frozen,
        }
    }

    fn keyring(&self) -> Keyring<'_> {
        Keyring::new(&self.github, &self.disk, &self.clock)
    }
}

#[test]
fn a_new_key_is_registered_and_usable() {
    let f = Fixture::new();
    let at = place();

    let key = f.keyring().create("david-lee-dev", &at, false, &Silent).unwrap();

    assert_eq!(key.state, KeyState::Registered);
    assert_eq!(key.remote_id.as_deref(), Some("remote-1"));
    assert_eq!(f.github.live(), vec!["remote-1"]);
}

#[test]
fn a_key_whose_registration_failed_keeps_its_private_half_and_says_so() {
    let f = Fixture::new();
    let at = place();
    *f.github.refuse_register.lock().unwrap() = true;

    assert!(f.keyring().create("david-lee-dev", &at, false, &Silent).is_err());

    // 개인 키를 버리지 않는다. 버리면 재시도가 불가능하고, 사용자는 왜 아무것도
    // 남지 않았는지 알 수 없다.
    let record = f.disk.record_of(&at);
    assert_eq!(record.state, KeyState::Local);
    assert!(record.remote_id.is_none());
    assert!(f.disk.exists(&at));
}

#[test]
fn retrying_registration_uses_the_key_already_on_disk() {
    let f = Fixture::new();
    let at = place();
    *f.github.refuse_register.lock().unwrap() = true;
    let _ = f.keyring().create("david-lee-dev", &at, false, &Silent);

    *f.github.refuse_register.lock().unwrap() = false;
    let key = f.keyring().retry(&at, &Silent).unwrap();

    assert_eq!(key.state, KeyState::Registered);
    // 같은 개인 키 그대로다. 새로 만들면 이미 배포해 둔 키가 못 쓰게 된다.
    assert_eq!(f.disk.public_key(&at).unwrap(), "pub-1");
}

#[test]
fn rotation_registers_the_new_key_before_removing_the_old_one() {
    let f = Fixture::new();
    let at = place();
    f.keyring().create("david-lee-dev", &at, false, &Silent).unwrap();

    // 옛 키를 지우지 못하는 상황 — 재발급이 중간에 멈춘다.
    *f.github.refuse_unregister.lock().unwrap() = true;
    assert!(f.keyring().rotate(&at, &Silent).is_err());

    // 새 키와 옛 키가 둘 다 살아 있다. 그래서 아무것도 끊기지 않는다.
    assert_eq!(f.github.live(), vec!["remote-1", "remote-2"]);
    let record = f.disk.record_of(&at);
    assert_eq!(record.state, KeyState::Rotating);
    assert_eq!(record.remote_id.as_deref(), Some("remote-2"));
    assert_eq!(record.retiring_remote_id.as_deref(), Some("remote-1"));
}

#[test]
fn a_stalled_rotation_finishes_where_it_stopped_without_making_another_key() {
    let f = Fixture::new();
    let at = place();
    f.keyring().create("david-lee-dev", &at, false, &Silent).unwrap();
    *f.github.refuse_unregister.lock().unwrap() = true;
    let _ = f.keyring().rotate(&at, &Silent);

    *f.github.refuse_unregister.lock().unwrap() = false;
    let key = f.keyring().rotate(&at, &Silent).unwrap();

    assert_eq!(key.state, KeyState::Registered);
    assert!(key.retiring_remote_id.is_none());
    // 이어서 진행했을 뿐, 세 번째 키를 만들지 않았다.
    assert_eq!(f.github.live(), vec!["remote-2"]);
}

#[test]
fn a_failed_rotation_leaves_the_working_key_in_place() {
    let f = Fixture::new();
    let at = place();
    f.keyring().create("david-lee-dev", &at, false, &Silent).unwrap();

    *f.github.refuse_register.lock().unwrap() = true;
    assert!(f.keyring().rotate(&at, &Silent).is_err());

    // 새 키를 등록하지 못했으면 제자리 키는 건드리지 않는다.
    assert_eq!(f.disk.public_key(&at).unwrap(), "pub-1");
    assert_eq!(f.disk.record_of(&at).state, KeyState::Registered);
    assert_eq!(f.github.live(), vec!["remote-1"]);
}

#[test]
fn removing_a_key_clears_github_before_the_record_is_archived() {
    let f = Fixture::new();
    let at = place();
    f.keyring().create("david-lee-dev", &at, false, &Silent).unwrap();

    *f.github.refuse_unregister.lock().unwrap() = true;
    assert!(f.keyring().remove(&at, &Silent).is_err());

    // GitHub 에서 못 지웠으면 기록도 남는다. 기록을 먼저 치우면 원격 id 를 잃어
    // 그 키를 다시는 지울 수 없다.
    assert!(f.disk.exists(&at));
    assert!(f.disk.archived.lock().unwrap().is_empty());

    *f.github.refuse_unregister.lock().unwrap() = false;
    f.keyring().remove(&at, &Silent).unwrap();
    assert!(f.github.live().is_empty());
    assert_eq!(f.disk.archived.lock().unwrap().len(), 1);
}

#[test]
fn a_stalled_rotation_removes_both_halves_from_github() {
    let f = Fixture::new();
    let at = place();
    f.keyring().create("david-lee-dev", &at, false, &Silent).unwrap();
    *f.github.refuse_unregister.lock().unwrap() = true;
    let _ = f.keyring().rotate(&at, &Silent);

    *f.github.refuse_unregister.lock().unwrap() = false;
    f.keyring().remove(&at, &Silent).unwrap();

    // 멈춘 재발급을 지우면 두 키가 다 사라져야 한다. 하나만 지우면 개인 키 없는
    // 등록이 GitHub 에 남는다.
    assert!(f.github.live().is_empty());
}

#[test]
fn a_key_cannot_be_created_where_one_already_sits() {
    let f = Fixture::new();
    let at = place();
    f.keyring().create("david-lee-dev", &at, false, &Silent).unwrap();

    assert!(matches!(
        f.keyring().create("david-lee-dev", &at, false, &Silent),
        Err(KeyError::Taken(_))
    ));
}

#[test]
fn changing_the_purpose_moves_the_key_without_touching_github() {
    let f = Fixture::new();
    let at = place();
    f.keyring().create("david-lee-dev", &at, false, &Silent).unwrap();

    let moved = KeyRef::new(RepoRef::parse("david-lee-dev/nemo-play").unwrap(), "for-ci").unwrap();
    let key = f.keyring().set_purpose(&at, "for-ci").unwrap();

    assert_eq!(key.purpose, "for-ci");
    assert_eq!(key.state, KeyState::Registered);
    // 같은 개인 키이고, 같은 등록이다. 원격은 건드리지 않는다.
    assert_eq!(f.disk.public_key(&moved).unwrap(), "pub-1");
    assert_eq!(key.remote_id.as_deref(), Some("remote-1"));
    assert_eq!(f.github.live(), vec!["remote-1"]);
    assert!(f.disk.load(&at).is_err());
}

#[test]
fn a_purpose_already_taken_in_the_same_repo_is_refused() {
    let f = Fixture::new();
    let at = place();
    let other = KeyRef::new(RepoRef::parse("david-lee-dev/nemo-play").unwrap(), "for-ci").unwrap();
    f.keyring().create("david-lee-dev", &at, false, &Silent).unwrap();
    f.keyring().create("david-lee-dev", &other, false, &Silent).unwrap();

    assert!(matches!(
        f.keyring().set_purpose(&at, "for-ci"),
        Err(KeyError::Taken(_))
    ));
    // 거절했으면 아무것도 건드리지 않았어야 한다.
    assert_eq!(f.disk.record_of(&at).state, KeyState::Registered);
    assert_eq!(f.github.live(), vec!["remote-1", "remote-2"]);
}
