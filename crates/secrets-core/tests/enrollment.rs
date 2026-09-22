//! 등록 절차 자체의 계약. CLI 도 파일시스템도 없이 포트의 가짜 구현으로 검사한다.
//!
//! 여기서 보는 것은 "무엇을 어떤 순서로 부르고, 실패하면 무엇을 되돌리는가"다.

use std::sync::Mutex;

use secrets_core::account::{Account, Provider, Replacement};
use secrets_core::credential::CredentialInput;
use secrets_core::identity::{AccountFacts, ObservedIdentity, Observation};
use secrets_core::port::{
    AccountGateway, AccountRegistry, Clock, GatewayError, LoginChallenge, PreparationId, Prepared,
    ProgressSink, RegistryError, Silent,
};
use secrets_core::enrollment::{Draft, Enrollment};
use secrets_core::credential::secret::Secret;

fn octocat() -> Observation {
    Observation {
        identity: ObservedIdentity::Github {
            login: "octocat".into(),
            user_id: "583231".into(),
            public_email: None,
        },
        facts: AccountFacts {
            expires: Some("2027-01-31".into()),
            scopes: vec!["repo".into()],
            ..AccountFacts::default()
        },
    }
}

#[derive(Default)]
struct FakeGateway {
    prepared: Mutex<Vec<(String, Observation)>>,
    discarded: Mutex<Vec<String>>,
    identity: Mutex<Option<Result<Observation, String>>>,
}

impl FakeGateway {
    fn with(observation: Observation) -> FakeGateway {
        let gateway = FakeGateway::default();
        gateway
            .prepared
            .lock()
            .unwrap()
            .push(("prep-1".into(), observation));
        gateway
    }

    fn id(&self) -> PreparationId {
        PreparationId::named("prep-1")
    }

    fn discarded(&self) -> Vec<String> {
        self.discarded.lock().unwrap().clone()
    }
}

impl AccountGateway for FakeGateway {
    fn prepare(
        &self,
        _provider: Provider,
        _credential: CredentialInput,
        _progress: &dyn ProgressSink,
    ) -> Result<Prepared, GatewayError> {
        unimplemented!("이 테스트는 확인이 끝난 상태에서 시작한다")
    }

    fn prepare_with_browser(
        &self,
        _provider: Provider,
        _progress: &dyn ProgressSink,
    ) -> Result<Prepared, GatewayError> {
        unimplemented!()
    }

    fn begin_browser_login(
        &self,
        _provider: Provider,
        _progress: &dyn ProgressSink,
    ) -> Result<(PreparationId, LoginChallenge), GatewayError> {
        unimplemented!()
    }

    fn complete_browser_login(
        &self,
        _id: &PreparationId,
        _code: &Secret,
        _progress: &dyn ProgressSink,
    ) -> Result<Prepared, GatewayError> {
        unimplemented!()
    }

    fn identity(
        &self,
        _account: &Account,
        _progress: &dyn ProgressSink,
    ) -> Result<Observation, GatewayError> {
        match self.identity.lock().unwrap().clone() {
            Some(Ok(observation)) => Ok(observation),
            Some(Err(why)) => Err(GatewayError::Rejected(why)),
            None => unimplemented!(),
        }
    }

    fn prepared(&self, id: &PreparationId) -> Option<Observation> {
        self.prepared
            .lock()
            .unwrap()
            .iter()
            .find(|(key, _)| key == id.as_str())
            .map(|(_, observation)| observation.clone())
    }

    fn discard(&self, id: &PreparationId) {
        self.discarded.lock().unwrap().push(id.as_str().to_string());
        self.prepared
            .lock()
            .unwrap()
            .retain(|(key, _)| key != id.as_str());
    }
}

#[derive(Default)]
struct FakeRegistry {
    stored: Mutex<Vec<Account>>,
    created_with: Mutex<Vec<String>>,
    refuse_create: Mutex<bool>,
    refuse_save: Mutex<bool>,
}

impl AccountRegistry for FakeRegistry {
    fn exists(&self, provider: Provider, slug: &str) -> bool {
        self.stored
            .lock()
            .unwrap()
            .iter()
            .any(|a| a.provider == provider && a.slug == slug)
    }

    fn load(&self, provider: Provider, slug: &str) -> Result<Account, RegistryError> {
        self.stored
            .lock()
            .unwrap()
            .iter()
            .find(|a| a.provider == provider && a.slug == slug)
            .cloned()
            .ok_or_else(|| RegistryError::NotFound(slug.into()))
    }

    fn list(&self) -> Vec<Result<Account, String>> {
        self.stored.lock().unwrap().iter().cloned().map(Ok).collect()
    }

    fn create(&self, account: &Account, prepared: &PreparationId) -> Result<(), RegistryError> {
        if *self.refuse_create.lock().unwrap() {
            return Err(RegistryError::Unwritable("쓸 수 없습니다".into()));
        }
        self.created_with
            .lock()
            .unwrap()
            .push(prepared.as_str().to_string());
        self.stored.lock().unwrap().push(account.clone());
        Ok(())
    }

    fn replace_credential(
        &self,
        account: &Account,
        _prepared: &PreparationId,
        _record: Replacement,
    ) -> Result<(), RegistryError> {
        self.stored
            .lock()
            .unwrap()
            .retain(|a| !(a.provider == account.provider && a.slug == account.slug));
        self.stored.lock().unwrap().push(account.clone());
        Ok(())
    }

    fn save(&self, account: &Account) -> Result<(), RegistryError> {
        if *self.refuse_save.lock().unwrap() {
            return Err(RegistryError::Unwritable("쓸 수 없습니다".into()));
        }
        self.stored
            .lock()
            .unwrap()
            .retain(|a| !(a.provider == account.provider && a.slug == account.slug));
        self.stored.lock().unwrap().push(account.clone());
        Ok(())
    }
}

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> String {
        "2026-09-22 10:00".into()
    }

    fn today(&self) -> String {
        "2026-09-22".into()
    }
}

fn draft(slug: &str) -> Draft {
    Draft {
        slug: slug.into(),
        display: "설명".into(),
        note: String::new(),
    }
}

#[test]
fn registering_hands_the_prepared_credential_to_the_registry() {
    let gateway = FakeGateway::with(octocat());
    let registry = FakeRegistry::default();
    let enrollment = Enrollment::new(&gateway, &registry, &FixedClock);

    let account = enrollment.register(&gateway.id(), draft("octocat")).unwrap();

    assert_eq!(account.identity.name, "octocat");
    assert_eq!(account.expires.as_deref(), Some("2027-01-31"));
    assert_eq!(
        account.git_email.as_deref(),
        Some("583231+octocat@users.noreply.github.com")
    );
    assert_eq!(
        *registry.created_with.lock().unwrap(),
        vec!["prep-1".to_string()],
        "레지스트리가 확인된 자격을 넘겨받아야 한다"
    );
    assert!(gateway.discarded().is_empty(), "성공한 준비를 버렸다");
}

#[test]
fn a_name_that_breaks_the_rules_never_reaches_the_registry() {
    let gateway = FakeGateway::with(octocat());
    let registry = FakeRegistry::default();
    let enrollment = Enrollment::new(&gateway, &registry, &FixedClock);

    enrollment
        .register(&gateway.id(), draft("Octo Cat"))
        .unwrap_err();

    assert!(registry.list().is_empty());
    assert_eq!(
        gateway.discarded(),
        vec!["prep-1".to_string()],
        "쓰지 못한 자격은 버려야 한다"
    );
}

#[test]
fn a_registry_that_refuses_leaves_no_prepared_credential_behind() {
    let gateway = FakeGateway::with(octocat());
    let registry = FakeRegistry::default();
    *registry.refuse_create.lock().unwrap() = true;
    let enrollment = Enrollment::new(&gateway, &registry, &FixedClock);

    enrollment
        .register(&gateway.id(), draft("octocat"))
        .unwrap_err();

    assert!(registry.list().is_empty());
    assert_eq!(gateway.discarded(), vec!["prep-1".to_string()]);
}

#[test]
fn a_preparation_can_only_be_used_once() {
    let gateway = FakeGateway::with(octocat());
    let registry = FakeRegistry::default();
    let enrollment = Enrollment::new(&gateway, &registry, &FixedClock);

    enrollment.register(&gateway.id(), draft("octocat")).unwrap();
    // 같은 표를 다시 쓰려 하면 이미 있는 계정이라 막히고, 자격도 남지 않는다.
    enrollment
        .register(&gateway.id(), draft("octocat"))
        .unwrap_err();
    assert_eq!(registry.list().len(), 1);
}

#[test]
fn a_failed_check_is_recorded_without_touching_the_identity() {
    let gateway = FakeGateway::with(octocat());
    let registry = FakeRegistry::default();
    let enrollment = Enrollment::new(&gateway, &registry, &FixedClock);
    let account = enrollment.register(&gateway.id(), draft("octocat")).unwrap();

    *gateway.identity.lock().unwrap() = Some(Err("토큰이 만료됐습니다".into()));
    let checked = enrollment.recheck(&account, &Silent).unwrap();

    let verification = checked.verification.as_ref().unwrap();
    assert!(!verification.ok);
    assert!(verification.detail.contains("토큰이 만료됐습니다"));
    assert_eq!(
        checked.identity.name, "octocat",
        "확인에 실패했다고 신원을 지우면 안 된다"
    );
}

#[test]
fn a_check_that_cannot_be_saved_is_a_failure() {
    let gateway = FakeGateway::with(octocat());
    let registry = FakeRegistry::default();
    let enrollment = Enrollment::new(&gateway, &registry, &FixedClock);
    let account = enrollment.register(&gateway.id(), draft("octocat")).unwrap();

    *gateway.identity.lock().unwrap() = Some(Ok(octocat()));
    *registry.refuse_save.lock().unwrap() = true;

    enrollment.recheck(&account, &Silent).unwrap_err();
}
