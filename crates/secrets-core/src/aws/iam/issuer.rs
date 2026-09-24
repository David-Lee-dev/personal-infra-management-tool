//! IAM 을 만들고, 소비처를 기록하고, 걷어내는 절차.
//!
//! 만들기는 AWS 와 금고에 걸쳐 여러 단계라, 도중에 실패하면 **손대기 전으로
//! 되돌린다.** 화면에 "만드는 중" 같은 상태를 두지 않기 위해서다. 되돌리기마저
//! 실패하면 무엇이 남았는지 진행 창에 그대로 적는다.

use crate::aws::iam::{
    Consumer, Env, IamError, IamGateway, IamName, IamRef, IamUser, IamVault, Naming, Policy,
    Sibling, UseCheck,
};
use crate::credential::secret::Secret;
use crate::port::{Channel, Clock, ProgressSink};
use crate::time;

/// 이만큼 넘게 쓰이지 않은 키만 지울 수 있다.
///
/// AWS 의 마지막 사용 시각은 UTC 날짜로 온다. 이 머신의 날짜와 하루 어긋날 수 있어,
/// 경계는 "넘게" — 31일째부터 — 로 둔다. 하루 늦게 지우는 편이 하루 일찍 끊는 것보다 낫다.
pub const IDLE_DAYS: i64 = 30;

/// 새 IAM 을 만들 때 받는 것.
pub struct Draft {
    /// 이 IAM 을 만들 마스터 계정.
    pub master: String,
    /// AWS 계정 ID.
    pub account: String,
    pub app: String,
    pub env: Env,
    /// 권한 조각. 비우면 정책에서 정한다.
    pub perm: Option<String>,
    pub purpose: String,
    pub policy: String,
}

pub struct Issuer<'a> {
    gateway: &'a dyn IamGateway,
    vault: &'a dyn IamVault,
    clock: &'a dyn Clock,
}

impl<'a> Issuer<'a> {
    pub fn new(
        gateway: &'a dyn IamGateway,
        vault: &'a dyn IamVault,
        clock: &'a dyn Clock,
    ) -> Issuer<'a> {
        Issuer {
            gateway,
            vault,
            clock,
        }
    }

    pub fn list(&self) -> Vec<Result<IamUser, String>> {
        self.vault.list()
    }

    pub fn load(&self, at: &IamRef) -> Result<IamUser, IamError> {
        self.vault.load(at)
    }

    /// 금고에 둔 정책 원문을 읽는다.
    pub fn policy(&self, at: &IamRef) -> Result<Policy, IamError> {
        Policy::read(&self.vault.policy(at)?).map_err(|e| IamError::Storage(e.0))
    }

    /// 이 정책으로 오늘 만들 IAM 의 이름.
    ///
    /// 권한 조각을 주지 않으면 정책에서 정한다 — 같은 앱 · 환경의 기존 IAM 과 대상을
    /// 견줘, 같으면 새 세대로, 다르면 다른 권한으로 본다. 같은 날 같은 이름이 이미
    /// 있으면 뒤에 번호를 붙인다.
    pub fn name_for(
        &self,
        account: &str,
        app: &str,
        env: Env,
        perm: Option<&str>,
        policy: &Policy,
    ) -> Result<IamName, IamError> {
        let perm = match perm.map(str::trim).filter(|p| !p.is_empty()) {
            Some(typed) => typed.to_string(),
            None => Naming::perm_for(policy, &self.siblings(account, app.trim(), env)).ok_or_else(|| {
                IamError::Invalid("권한 이름을 정할 수 없습니다 — 직접 적으세요".into())
            })?,
        };
        let name = IamName::compose(app, env, &perm, &self.clock.today()).ok_or_else(|| {
            IamError::Invalid(format!(
                "이름으로 쓸 수 없습니다: {}-{}-{perm}-iam (소문자 · 숫자 · - 만, 64자까지)",
                app.trim(),
                env.id()
            ))
        })?;

        let taken = self.vault.names(account);
        if !taken.contains(&name.full()) {
            return Ok(name);
        }
        (2..=9)
            .filter_map(|n| name.numbered(n))
            .find(|next| !taken.contains(&next.full()))
            .ok_or_else(|| IamError::Taken(name.full()))
    }

    /// 같은 앱 · 같은 환경에 이미 있는 IAM 과 그 대상. 정책을 읽지 못한 것은 대상이
    /// 없는 것으로 본다 — 그러면 새 세대로 오인하지 않고 다른 권한으로 갈라진다.
    fn siblings(&self, account: &str, app: &str, env: Env) -> Vec<Sibling> {
        self.vault
            .list()
            .into_iter()
            .flatten()
            .filter(|user| user.account == account && user.app == app && user.env == env.id())
            .map(|user| Sibling {
                resources: self
                    .policy(&user.at())
                    .map(|p| p.resources().into_iter().map(str::to_string).collect())
                    .unwrap_or_default(),
                perm: user.perm,
            })
            .collect()
    }

    /// 새 IAM. 사용자 · 정책 · 키를 만들고, 정책이 적힌 대로 도는지와 키가 들어가지는지
    /// 확인한다. 어디서든 실패하면 AWS 의 사용자를 지우고 금고를 비운다.
    pub fn create(&self, draft: &Draft, progress: &dyn ProgressSink) -> Result<IamUser, IamError> {
        let policy = Policy::read(&draft.policy).map_err(|e| IamError::Invalid(e.0))?;
        let name = self.name_for(
            &draft.account,
            &draft.app,
            draft.env,
            draft.perm.as_deref(),
            &policy,
        )?;
        let at = IamRef {
            account: draft.account.clone(),
            name: name.full(),
        };
        if self.vault.exists(&at) {
            return Err(IamError::Taken(at.name));
        }

        self.gateway.create_user(&draft.master, &at.name, progress)?;
        match self.finish_creating(draft, &name, &policy, progress) {
            Ok(user) => Ok(user),
            Err(e) => {
                self.undo_user(&draft.master, &at.name, progress);
                self.vault.discard(&at);
                Err(e)
            }
        }
    }

    fn finish_creating(
        &self,
        draft: &Draft,
        name: &IamName,
        policy: &Policy,
        progress: &dyn ProgressSink,
    ) -> Result<IamUser, IamError> {
        let full = name.full();
        let at = IamRef {
            account: draft.account.clone(),
            name: full.clone(),
        };
        self.gateway
            .put_policy(&draft.master, &full, &policy.text, progress)?;
        self.check_probes(&draft.master, &at, policy, progress)?;

        let (key_id, secret) = self.gateway.issue_key(&draft.master, &full, progress)?;
        let now = self.clock.now();
        // 한 번도 쓰이지 않은 키는 발급 시각이 기준이다. 묻지 않고도 하한을 안다.
        let deletable_from = Issuer::deletable_after(&now).unwrap_or_default();
        let user = IamUser {
            name: full,
            app: name.app.clone(),
            env: name.env.id().to_string(),
            perm: name.perm.clone(),
            purpose: draft.purpose.trim().to_string(),
            account: draft.account.clone(),
            master: draft.master.clone(),
            key_id,
            issued_at: now.clone(),
            created_at: now,
            consumers: Vec::new(),
            checked: None,
            deletable_from,
        };
        // 시크릿은 지금 한 번만 받을 수 있다. 확인보다 먼저 금고에 둔다.
        self.vault.keep(&user, &policy.text, &secret)?;
        self.confirm_key(&user, &user.key_id, &secret, progress)?;
        Ok(user)
    }

    /// 정책이 적힌 대로 도는지 AWS 의 시뮬레이터에 묻는다.
    fn check_probes(
        &self,
        master: &str,
        at: &IamRef,
        policy: &Policy,
        progress: &dyn ProgressSink,
    ) -> Result<(), IamError> {
        for probe in policy.probes() {
            let allowed = self.gateway.allows(master, at, &probe, progress)?;
            if allowed != probe.allowed {
                return Err(IamError::Probe(format!(
                    "{} {} 이(가) {} — {}",
                    probe.action,
                    probe.resource,
                    if allowed { "허용됩니다" } else { "거부됩니다" },
                    if probe.allowed { "허용돼야 합니다" } else { "거부돼야 합니다" },
                )));
            }
            progress.line(
                Channel::Out,
                &format!(
                    "{} {} → {}",
                    probe.action,
                    probe.resource,
                    if allowed { "허용" } else { "거부" }
                ),
            );
        }
        Ok(())
    }

    /// 그 키로 AWS 에 들어가, 주인이 이 IAM 인지 본다.
    fn confirm_key(
        &self,
        user: &IamUser,
        key_id: &str,
        secret: &Secret,
        progress: &dyn ProgressSink,
    ) -> Result<(), IamError> {
        let arn = self.gateway.identify(key_id, secret, progress)?;
        if !arn.ends_with(&format!(":user/{}", user.name)) {
            return Err(IamError::Probe(format!("키의 주인이 {arn} 입니다")));
        }
        Ok(())
    }

    fn undo_user(&self, master: &str, name: &str, progress: &dyn ProgressSink) {
        if let Err(e) = self.gateway.delete_user(master, name, progress) {
            progress.line(
                Channel::Err,
                &format!("되돌리지 못했습니다 — AWS 에 {name} 이(가) 남아 있습니다: {e}"),
            );
        }
    }

    /// 시크릿. `.env` 에 붙여 넣으려고 꺼낼 때만 쓴다.
    pub fn secret(&self, at: &IamRef) -> Result<Secret, IamError> {
        self.vault.secret(at)
    }

    /// 이 키를 어디에 넣었는지 적는다. 파일은 건드리지 않는다 — 넣는 일은 사람이 한다.
    pub fn add_consumer(
        &self,
        at: &IamRef,
        host: &str,
        file: &str,
        id_variable: &str,
    ) -> Result<IamUser, IamError> {
        let mut user = self.vault.load(at)?;
        let consumer = self.consumer(host, file, id_variable)?;
        self.ensure_free(&consumer)?;

        user.consumers.push(consumer);
        self.vault.record(&user)?;
        Ok(user)
    }

    fn consumer(&self, host: &str, file: &str, id_variable: &str) -> Result<Consumer, IamError> {
        let (host, file) = (host.trim(), file.trim());
        if host.is_empty() || file.is_empty() {
            return Err(IamError::Invalid("호스트와 파일을 적으세요".into()));
        }
        let secret_variable = Consumer::secret_variable_for(id_variable).ok_or_else(|| {
            IamError::Invalid(format!(
                "변수 이름은 대문자 · 숫자 · _ 로, …ACCESS_KEY_ID 로 끝나야 합니다: {id_variable}"
            ))
        })?;
        Ok(Consumer {
            host: host.to_string(),
            file: file.to_string(),
            id_variable: id_variable.trim().to_string(),
            secret_variable,
            recorded_at: self.clock.now(),
        })
    }

    /// 같은 파일의 같은 변수를 두 IAM 이 차지한다고 적지 않는다. 한 자리에는 키
    /// 하나만 들어갈 수 있으니, 둘 중 하나의 기록은 거짓이 된다.
    fn ensure_free(&self, consumer: &Consumer) -> Result<(), IamError> {
        for other in self.vault.list().into_iter().flatten() {
            if other.consumers.iter().any(|c| c.same_place(consumer)) {
                return Err(IamError::Taken(format!(
                    "{} — {} 의 기록에 있는 자리",
                    consumer.slug(),
                    other.name
                )));
            }
        }
        Ok(())
    }

    /// 기록에서 뺀다. 파일은 건드리지 않는다.
    pub fn remove_consumer(
        &self,
        at: &IamRef,
        host: &str,
        file: &str,
        id_variable: &str,
    ) -> Result<IamUser, IamError> {
        let mut user = self.vault.load(at)?;
        let index = user
            .consumers
            .iter()
            .position(|c| c.host == host && c.file == file && c.id_variable == id_variable)
            .ok_or_else(|| IamError::Missing(format!("{host}:{file} {id_variable}")))?;
        user.consumers.remove(index);
        self.vault.record(&user)?;
        Ok(user)
    }

    /// IAM 을 걷어낸다. 키가 [`IDLE_DAYS`] 일 넘게 쓰이지 않았을 때만 한다.
    ///
    /// 소비처 기록은 사람이 적는 것이라 빠질 수 있다. 그래서 기록이 아니라 AWS 가 본
    /// 마지막 사용을 기준으로 막는다. 막힐 때도 본 것은 기록한다 — 삭제 가능일이
    /// 그 자리에서 갱신된다.
    ///
    /// AWS 에서 먼저 지우고 실물은 보관한다. 기록을 먼저 치우면 무엇을 지워야 하는지 잃는다.
    pub fn remove(&self, at: &IamRef, progress: &dyn ProgressSink) -> Result<(), IamError> {
        let mut user = self.vault.load(at)?;
        let (idle_days, last) = self.observe(&mut user, progress)?;
        if idle_days <= IDLE_DAYS {
            return Err(IamError::Recent {
                idle_days,
                last,
                from: user.deletable_from,
            });
        }

        self.gateway.delete_user(&user.master, &user.name, progress)?;
        self.vault.archive(at, "삭제")
    }

    /// AWS 에 마지막 사용을 묻고 기록한다. 삭제 가능일도 여기서 다시 정해진다.
    pub fn last_used(&self, at: &IamRef, progress: &dyn ProgressSink) -> Result<IamUser, IamError> {
        let mut user = self.vault.load(at)?;
        self.observe(&mut user, progress)?;
        Ok(user)
    }

    /// 마지막 사용을 묻고, 본 것과 삭제 가능일을 적는다. 돌려주는 것은 (쉰 날수, 기준 시각).
    ///
    /// 한 번도 쓰이지 않았으면 발급 시각이 기준이다. 시각을 읽지 못하면 아무것도
    /// 적지 않고 실패한다 — 틀린 날짜를 적으면 이르게 지울 수 있다.
    fn observe(&self, user: &mut IamUser, progress: &dyn ProgressSink) -> Result<(i64, String), IamError> {
        let used = self.gateway.last_used(&user.master, &user.key_id, progress)?;
        let last = used
            .as_ref()
            .map(|u| u.at.clone())
            .unwrap_or_else(|| user.issued_at.clone());
        let unreadable =
            || IamError::Invalid(format!("마지막 사용 시각을 읽지 못해 지우지 않습니다: {last}"));
        let idle_days = Issuer::idle_days(&last, &self.clock.today()).ok_or_else(unreadable)?;
        let deletable_from = Issuer::deletable_after(&last).ok_or_else(unreadable)?;

        user.checked = Some(UseCheck {
            checked_at: self.clock.now(),
            last: used,
        });
        user.deletable_from = deletable_from;
        self.vault.record(user)?;
        Ok((idle_days, last))
    }

    /// 그 시각부터 오늘까지 며칠. 시각은 ISO 8601 이고 날짜 부분만 본다.
    fn idle_days(since: &str, today: &str) -> Option<i64> {
        time::days_between(since.get(..10)?, today)
    }

    /// 그 시각에 마지막으로 쓰였다면 지울 수 있게 되는 날.
    fn deletable_after(since: &str) -> Option<String> {
        time::plus_days(since.get(..10)?, IDLE_DAYS + 1)
    }

    pub fn set_purpose(&self, at: &IamRef, to: &str) -> Result<IamUser, IamError> {
        let mut user = self.vault.load(at)?;
        user.purpose = to.trim().to_string();
        self.vault.record(&user)?;
        Ok(user)
    }
}
