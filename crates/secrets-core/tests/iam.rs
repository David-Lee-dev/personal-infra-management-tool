//! IAM 절차가 도중에 실패하면 무엇이 남는가, 그리고 아직 쓰이는 키가 지워지지 않는가.
//!
//! 만들기는 AWS 와 금고에 걸친 여러 단계라, 실패하면 손대기 전으로 되돌아가는지를
//! 본다. 삭제는 AWS 가 본 마지막 사용을 기준으로 막히는지를 본다.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use secrets_core::aws::iam::{
    Draft, Env, IamError, IamGateway, IamRef, IamUser, IamVault, Issuer, LastUse, Probe,
};
use secrets_core::credential::secret::Secret;
use secrets_core::port::{Clock, ProgressSink, Silent};

const MASTER: &str = "admin";
const ACCOUNT: &str = "123456789012";

struct Frozen;

impl Clock for Frozen {
    fn now(&self) -> String {
        "2026-09-24T10:00:00+09:00".into()
    }
    fn today(&self) -> String {
        "2026-09-24".into()
    }
}

/* ── 가짜 AWS ───────────────────────────────────────── */

/// 붙은 정책과, 키 id → 시크릿.
type Account = (Option<String>, BTreeMap<String, String>);

#[derive(Default)]
struct Aws {
    /// 사용자 → 그 사용자의 정책과 키.
    users: Mutex<BTreeMap<String, Account>>,
    issued: Mutex<u32>,
    /// 시뮬레이터가 이 동작에 대해 거꾸로 답한다.
    lie_about: Mutex<Option<String>>,
    refuse_identify: Mutex<bool>,
    /// 키가 마지막으로 쓰인 시각. 없으면 한 번도 쓰이지 않았다.
    used_at: Mutex<Option<String>>,
}

impl Aws {
    fn keys_of(&self, name: &str) -> Vec<String> {
        self.users
            .lock()
            .unwrap()
            .get(name)
            .map(|(_, keys)| keys.keys().cloned().collect())
            .unwrap_or_default()
    }

    fn has_user(&self, name: &str) -> bool {
        self.users.lock().unwrap().contains_key(name)
    }
}

impl IamGateway for Aws {
    fn account_id(&self, _master: &str, _p: &dyn ProgressSink) -> Result<String, IamError> {
        Ok(ACCOUNT.into())
    }

    fn create_user(&self, _m: &str, name: &str, _p: &dyn ProgressSink) -> Result<(), IamError> {
        let mut users = self.users.lock().unwrap();
        if users.contains_key(name) {
            return Err(IamError::Remote("EntityAlreadyExists".into()));
        }
        users.insert(name.into(), (None, BTreeMap::new()));
        Ok(())
    }

    fn put_policy(&self, _m: &str, name: &str, policy: &str, _p: &dyn ProgressSink) -> Result<(), IamError> {
        let mut users = self.users.lock().unwrap();
        let entry = users.get_mut(name).ok_or_else(|| IamError::Remote("NoSuchEntity".into()))?;
        entry.0 = Some(policy.into());
        Ok(())
    }

    fn allows(&self, _m: &str, _at: &IamRef, probe: &Probe, _p: &dyn ProgressSink) -> Result<bool, IamError> {
        let lie = self.lie_about.lock().unwrap().as_deref() == Some(probe.action.as_str());
        Ok(probe.allowed != lie)
    }

    fn issue_key(&self, _m: &str, name: &str, _p: &dyn ProgressSink) -> Result<(String, Secret), IamError> {
        let mut users = self.users.lock().unwrap();
        let entry = users.get_mut(name).ok_or_else(|| IamError::Remote("NoSuchEntity".into()))?;
        if entry.1.len() >= 2 {
            return Err(IamError::Remote("LimitExceeded".into()));
        }
        let mut n = self.issued.lock().unwrap();
        *n += 1;
        let (id, secret) = (format!("KEY{n}"), format!("secret{n}"));
        entry.1.insert(id.clone(), secret.clone());
        Ok((id, Secret::new(secret)))
    }

    fn identify(&self, key_id: &str, secret: &Secret, _p: &dyn ProgressSink) -> Result<String, IamError> {
        if *self.refuse_identify.lock().unwrap() {
            return Err(IamError::Remote("InvalidClientTokenId".into()));
        }
        let users = self.users.lock().unwrap();
        users
            .iter()
            .find(|(_, (_, keys))| keys.get(key_id).map(String::as_str) == Some(secret.expose()))
            .map(|(name, _)| format!("arn:aws:iam::{ACCOUNT}:user/{name}"))
            .ok_or_else(|| IamError::Remote("InvalidClientTokenId".into()))
    }

    fn delete_user(&self, _m: &str, name: &str, _p: &dyn ProgressSink) -> Result<(), IamError> {
        self.users.lock().unwrap().remove(name);
        Ok(())
    }

    fn last_used(&self, _m: &str, _k: &str, _p: &dyn ProgressSink) -> Result<Option<LastUse>, IamError> {
        Ok(self.used_at.lock().unwrap().clone().map(|at| LastUse {
            at,
            service: "s3".into(),
            region: "ap-northeast-2".into(),
        }))
    }
}

/* ── 가짜 금고 ──────────────────────────────────────── */

#[derive(Default)]
struct Vault {
    users: Mutex<BTreeMap<String, IamUser>>,
    policies: Mutex<HashMap<String, String>>,
    secrets: Mutex<HashMap<String, String>>,
    archived: Mutex<Vec<String>>,
}

impl Vault {
    fn secret_of(&self, name: &str) -> Option<String> {
        self.secrets.lock().unwrap().get(name).cloned()
    }
}

impl IamVault for Vault {
    fn exists(&self, at: &IamRef) -> bool {
        self.users.lock().unwrap().contains_key(&at.name)
    }
    fn names(&self, _account: &str) -> Vec<String> {
        self.users.lock().unwrap().keys().cloned().collect()
    }
    fn list(&self) -> Vec<Result<IamUser, String>> {
        self.users.lock().unwrap().values().cloned().map(Ok).collect()
    }
    fn load(&self, at: &IamRef) -> Result<IamUser, IamError> {
        self.users
            .lock()
            .unwrap()
            .get(&at.name)
            .cloned()
            .ok_or_else(|| IamError::Missing(at.slug()))
    }
    fn record(&self, user: &IamUser) -> Result<(), IamError> {
        self.users.lock().unwrap().insert(user.name.clone(), user.clone());
        Ok(())
    }
    fn keep(&self, user: &IamUser, policy: &str, secret: &Secret) -> Result<(), IamError> {
        self.record(user)?;
        self.policies.lock().unwrap().insert(user.name.clone(), policy.into());
        self.secrets.lock().unwrap().insert(user.name.clone(), secret.expose().into());
        Ok(())
    }
    fn policy(&self, at: &IamRef) -> Result<String, IamError> {
        self.policies.lock().unwrap().get(&at.name).cloned().ok_or_else(|| IamError::Missing(at.slug()))
    }
    fn secret(&self, at: &IamRef) -> Result<Secret, IamError> {
        self.secret_of(&at.name).map(Secret::new).ok_or_else(|| IamError::Missing(at.slug()))
    }
    fn archive(&self, at: &IamRef, _reason: &str) -> Result<(), IamError> {
        self.users.lock().unwrap().remove(&at.name);
        self.archived.lock().unwrap().push(at.name.clone());
        Ok(())
    }
    fn discard(&self, at: &IamRef) {
        self.users.lock().unwrap().remove(&at.name);
        self.policies.lock().unwrap().remove(&at.name);
        self.secrets.lock().unwrap().remove(&at.name);
    }
}

/* ── 조립 ───────────────────────────────────────────── */

const S3_POLICY: &str = r#"{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":["s3:PutObject"],"Resource":"arn:aws:s3:::bucket/avatars/*"}]}"#;

fn draft(policy: &str) -> Draft {
    Draft {
        master: MASTER.into(),
        account: ACCOUNT.into(),
        app: "tuk-api".into(),
        env: Env::Prod,
        perm: None,
        purpose: "이미지 업로드".into(),
        policy: policy.into(),
    }
}

fn at(name: &str) -> IamRef {
    IamRef { account: ACCOUNT.into(), name: name.into() }
}

const NAME: &str = "tuk-api-prod-s3-iam-20260924";

/* ── 만들기 ─────────────────────────────────────────── */

mod create {
    use super::*;

    #[test]
    fn makes_the_user_with_its_policy_and_one_key_and_keeps_the_secret() {
        let (aws, vault) = (Aws::default(), Vault::default());
        let issuer = Issuer::new(&aws, &vault, &Frozen);

        let user = issuer.create(&draft(S3_POLICY), &Silent).unwrap();

        assert_eq!(user.name, NAME);
        assert_eq!(user.perm, "s3");
        assert_eq!(aws.users.lock().unwrap()[NAME].0.as_deref(), Some(S3_POLICY));
        assert_eq!(aws.keys_of(NAME), vec![user.key_id.clone()]);
        assert_eq!(vault.secret_of(NAME).as_deref(), Some("secret1"));
    }

    #[test]
    fn a_new_key_can_be_deleted_thirty_one_days_after_it_was_issued() {
        let (aws, vault) = (Aws::default(), Vault::default());
        let issuer = Issuer::new(&aws, &vault, &Frozen);

        let user = issuer.create(&draft(S3_POLICY), &Silent).unwrap();

        assert_eq!(user.deletable_from, "2026-10-25");
        assert!(user.checked.is_none(), "묻지 않았는데 물은 것으로 적혔다");
    }

    #[test]
    fn a_second_policy_on_the_same_service_is_named_after_its_resource() {
        let (aws, vault) = (Aws::default(), Vault::default());
        let issuer = Issuer::new(&aws, &vault, &Frozen);
        issuer.create(&draft(S3_POLICY), &Silent).unwrap();

        let logs = r#"{"Statement":[{"Effect":"Allow","Action":"s3:PutObject","Resource":"arn:aws:s3:::logs/applog-archive/*"}]}"#;
        let user = issuer.create(&draft(logs), &Silent).unwrap();

        assert_eq!(user.name, "tuk-api-prod-s3-applog-archive-iam-20260924");
    }

    #[test]
    fn the_same_policy_again_is_a_new_generation_numbered_on_the_same_day() {
        let (aws, vault) = (Aws::default(), Vault::default());
        let issuer = Issuer::new(&aws, &vault, &Frozen);
        issuer.create(&draft(S3_POLICY), &Silent).unwrap();

        let again = issuer.create(&draft(S3_POLICY), &Silent).unwrap();

        assert_eq!(again.name, "tuk-api-prod-s3-iam-20260924-2");
        assert_eq!(again.perm, "s3");
        assert!(aws.has_user(NAME), "옛 세대가 건드려졌다");
    }

    #[test]
    fn a_policy_that_does_not_do_what_it_says_leaves_nothing_behind() {
        let (aws, vault) = (Aws::default(), Vault::default());
        *aws.lie_about.lock().unwrap() = Some("iam:CreateUser".into());
        let issuer = Issuer::new(&aws, &vault, &Frozen);

        let err = issuer.create(&draft(S3_POLICY), &Silent).unwrap_err();

        assert!(matches!(err, IamError::Probe(_)), "{err}");
        assert!(!aws.has_user(NAME), "AWS 에 사용자가 남았다");
        assert!(!vault.exists(&at(NAME)), "금고에 기록이 남았다");
    }

    #[test]
    fn a_key_that_cannot_log_in_leaves_nothing_behind() {
        let (aws, vault) = (Aws::default(), Vault::default());
        *aws.refuse_identify.lock().unwrap() = true;
        let issuer = Issuer::new(&aws, &vault, &Frozen);

        issuer.create(&draft(S3_POLICY), &Silent).unwrap_err();

        assert!(!aws.has_user(NAME));
        assert!(!vault.exists(&at(NAME)));
        assert_eq!(vault.secret_of(NAME), None, "시크릿이 남았다");
    }

    #[test]
    fn broken_json_is_refused_before_aws_is_touched() {
        let (aws, vault) = (Aws::default(), Vault::default());
        let issuer = Issuer::new(&aws, &vault, &Frozen);

        let err = issuer.create(&draft("{"), &Silent).unwrap_err();

        assert!(matches!(err, IamError::Invalid(_)));
        assert!(aws.users.lock().unwrap().is_empty());
    }
}

/* ── 소비처 ─────────────────────────────────────────── */

mod consumers {
    use super::*;

    #[test]
    fn adding_records_the_place_with_its_paired_secret_variable() {
        let (aws, vault) = (Aws::default(), Vault::default());
        let issuer = Issuer::new(&aws, &vault, &Frozen);
        issuer.create(&draft(S3_POLICY), &Silent).unwrap();

        let after = issuer
            .add_consumer(&at(NAME), "tukapp-prod", "~/back/.env", "AWS_S3_ACCESS_KEY_ID")
            .unwrap();

        let kept = &vault.load(&at(NAME)).unwrap().consumers;
        assert_eq!(after.consumers.len(), 1);
        assert_eq!(kept[0].secret_variable, "AWS_S3_SECRET_ACCESS_KEY");
        assert_eq!(kept[0].recorded_at, "2026-09-24T10:00:00+09:00");
    }

    #[test]
    fn a_place_already_recorded_for_another_iam_is_refused() {
        let (aws, vault) = (Aws::default(), Vault::default());
        let issuer = Issuer::new(&aws, &vault, &Frozen);
        issuer.create(&draft(S3_POLICY), &Silent).unwrap();
        issuer
            .add_consumer(&at(NAME), "tukapp-prod", "~/back/.env", "AWS_S3_ACCESS_KEY_ID")
            .unwrap();
        let logs = r#"{"Statement":[{"Effect":"Allow","Action":"s3:PutObject","Resource":"arn:aws:s3:::logs/applog-archive/*"}]}"#;
        let second = issuer.create(&draft(logs), &Silent).unwrap();

        let err = issuer
            .add_consumer(&second.at(), "tukapp-prod", "~/back/.env", "AWS_S3_ACCESS_KEY_ID")
            .unwrap_err();

        assert!(matches!(err, IamError::Taken(_)), "{err}");
        assert!(vault.load(&second.at()).unwrap().consumers.is_empty());
    }

    #[test]
    fn a_variable_outside_the_naming_rule_is_refused() {
        let (aws, vault) = (Aws::default(), Vault::default());
        let issuer = Issuer::new(&aws, &vault, &Frozen);
        issuer.create(&draft(S3_POLICY), &Silent).unwrap();

        let err = issuer.add_consumer(&at(NAME), "local", "~/.env", "S3_KEY").unwrap_err();

        assert!(matches!(err, IamError::Invalid(_)));
    }

    #[test]
    fn removing_takes_it_out_of_the_record_only() {
        let (aws, vault) = (Aws::default(), Vault::default());
        let issuer = Issuer::new(&aws, &vault, &Frozen);
        issuer.create(&draft(S3_POLICY), &Silent).unwrap();
        issuer.add_consumer(&at(NAME), "local", "~/.env", "AWS_ACCESS_KEY_ID").unwrap();

        let after = issuer
            .remove_consumer(&at(NAME), "local", "~/.env", "AWS_ACCESS_KEY_ID")
            .unwrap();

        assert!(after.consumers.is_empty());
        assert!(aws.has_user(NAME), "기록을 빼는 일이 AWS 를 건드렸다");
    }
}

/* ── 걷어내기 ───────────────────────────────────────── */

mod remove {
    use super::*;

    fn made() -> (Aws, Vault) {
        let (aws, vault) = (Aws::default(), Vault::default());
        Issuer::new(&aws, &vault, &Frozen)
            .create(&draft(S3_POLICY), &Silent)
            .unwrap();
        (aws, vault)
    }

    #[test]
    fn a_key_idle_for_more_than_thirty_days_is_removed_and_archived() {
        let (aws, vault) = made();
        *aws.used_at.lock().unwrap() = Some("2026-08-24T05:33:00+00:00".into());
        let issuer = Issuer::new(&aws, &vault, &Frozen);

        issuer.remove(&at(NAME), &Silent).unwrap();

        assert!(!aws.has_user(NAME));
        assert_eq!(*vault.archived.lock().unwrap(), vec![NAME.to_string()]);
    }

    #[test]
    fn a_key_used_exactly_thirty_days_ago_is_kept() {
        let (aws, vault) = made();
        *aws.used_at.lock().unwrap() = Some("2026-08-25T05:33:00+00:00".into());
        let issuer = Issuer::new(&aws, &vault, &Frozen);

        let err = issuer.remove(&at(NAME), &Silent).unwrap_err();

        assert!(matches!(err, IamError::Recent { idle_days: 30, ref from, .. } if from == "2026-09-25"), "{err}");
        assert!(aws.has_user(NAME));
        assert!(vault.archived.lock().unwrap().is_empty());
    }

    #[test]
    fn a_refusal_moves_the_deletable_date_to_what_aws_just_said() {
        let (aws, vault) = made();
        *aws.used_at.lock().unwrap() = Some("2026-09-20T01:00:00+00:00".into());
        let issuer = Issuer::new(&aws, &vault, &Frozen);

        issuer.remove(&at(NAME), &Silent).unwrap_err();

        let kept = vault.load(&at(NAME)).unwrap();
        assert_eq!(kept.deletable_from, "2026-10-21");
        let checked = kept.checked.expect("본 것이 기록되지 않았다");
        assert_eq!(checked.checked_at, "2026-09-24T10:00:00+09:00");
        assert_eq!(checked.last.unwrap().at, "2026-09-20T01:00:00+00:00");
    }

    #[test]
    fn asking_for_the_last_use_records_it_even_when_never_used() {
        let (aws, vault) = made();
        let issuer = Issuer::new(&aws, &vault, &Frozen);

        let user = issuer.last_used(&at(NAME), &Silent).unwrap();

        assert!(user.checked.as_ref().unwrap().last.is_none());
        assert_eq!(user.deletable_from, "2026-10-25");
        assert!(vault.load(&at(NAME)).unwrap().checked.is_some());
    }

    #[test]
    fn a_never_used_key_counts_from_when_it_was_issued() {
        let (aws, vault) = made();
        let issuer = Issuer::new(&aws, &vault, &Frozen);

        let err = issuer.remove(&at(NAME), &Silent).unwrap_err();
        assert!(matches!(err, IamError::Recent { idle_days: 0, .. }), "{err}");

        let mut old = vault.load(&at(NAME)).unwrap();
        old.issued_at = "2026-07-01T09:00:00+09:00".into();
        vault.record(&old).unwrap();
        issuer.remove(&at(NAME), &Silent).unwrap();
        assert!(!aws.has_user(NAME));
    }

    #[test]
    fn recorded_consumers_do_not_decide_it_the_last_use_does() {
        let (aws, vault) = made();
        let issuer = Issuer::new(&aws, &vault, &Frozen);
        issuer.add_consumer(&at(NAME), "local", "~/.env", "AWS_ACCESS_KEY_ID").unwrap();
        *aws.used_at.lock().unwrap() = Some("2026-06-01T00:00:00+00:00".into());

        issuer.remove(&at(NAME), &Silent).unwrap();

        assert!(!aws.has_user(NAME));
    }

    #[test]
    fn an_unreadable_time_keeps_the_key() {
        let (aws, vault) = made();
        *aws.used_at.lock().unwrap() = Some("어제".into());
        let issuer = Issuer::new(&aws, &vault, &Frozen);

        let err = issuer.remove(&at(NAME), &Silent).unwrap_err();

        assert!(matches!(err, IamError::Invalid(_)), "{err}");
        assert!(aws.has_user(NAME));
        assert_eq!(vault.load(&at(NAME)).unwrap().deletable_from, "2026-10-25", "틀린 날짜가 적혔다");
    }
}
