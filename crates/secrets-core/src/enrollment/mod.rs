//! 계정 등록·교체·재확인. 이 도구의 핵심 절차다.
//!
//! 세 가지가 모두 같은 규칙을 따른다.
//!
//! - **확인이 먼저다.** 로그인이 되는 자격만 레지스트리에 들어간다.
//! - **사실은 관찰에서만 온다.** 신원·권한·만료일을 호출자가 적어 넣을 자리가 없다.
//! - **중간 상태를 남기지 않는다.** 실패하면 손대기 전과 구별되지 않아야 한다.

mod draft;
mod error;

pub use draft::Draft;
pub use error::EnrollError;

use crate::account::{self, Account, ArchiveReason, Provider, Replacement};
use crate::credential::CredentialInput;
use crate::identity::{Observation, same_account};
use crate::port::{
    AccountGateway, AccountRegistry, Clock, LoginChallenge, PreparationId, Prepared,
    ProgressSink, RegistryError,
};
use crate::credential::secret::Secret;

/// 등록 절차. 필요한 바깥 동작을 포트로 받아 쥔다.
pub struct Enrollment<'a> {
    gateway: &'a dyn AccountGateway,
    registry: &'a dyn AccountRegistry,
    clock: &'a dyn Clock,
}

impl<'a> Enrollment<'a> {
    pub fn new(
        gateway: &'a dyn AccountGateway,
        registry: &'a dyn AccountRegistry,
        clock: &'a dyn Clock,
    ) -> Enrollment<'a> {
        Enrollment {
            gateway,
            registry,
            clock,
        }
    }

    /// 받아 적은 자격으로 로그인해 누구인지 확인한다.
    pub fn check(
        &self,
        provider: Provider,
        credential: CredentialInput,
        progress: &dyn ProgressSink,
    ) -> Result<Prepared, EnrollError> {
        if let Some(field) = credential.missing() {
            return Err(EnrollError::Missing(field));
        }
        Ok(self.gateway.prepare(provider, credential, progress)?)
    }

    /// 브라우저로 한 번에 끝나는 로그인.
    pub fn check_with_browser(
        &self,
        provider: Provider,
        progress: &dyn ProgressSink,
    ) -> Result<Prepared, EnrollError> {
        Ok(self.gateway.prepare_with_browser(provider, progress)?)
    }

    pub fn begin_browser_login(
        &self,
        provider: Provider,
        progress: &dyn ProgressSink,
    ) -> Result<(PreparationId, LoginChallenge), EnrollError> {
        Ok(self.gateway.begin_browser_login(provider, progress)?)
    }

    pub fn complete_browser_login(
        &self,
        id: &PreparationId,
        code: &Secret,
        progress: &dyn ProgressSink,
    ) -> Result<Prepared, EnrollError> {
        Ok(self.gateway.complete_browser_login(id, code, progress)?)
    }

    /// 확인만 하고 쓰지 않기로 한 자격을 버린다.
    pub fn discard(&self, id: &PreparationId) {
        self.gateway.discard(id);
    }

    /// 확인된 자격을 계정으로 확정한다.
    ///
    /// 실패하면 준비한 자격은 버려지고 레지스트리는 손대기 전 그대로다.
    pub fn register(&self, id: &PreparationId, draft: Draft) -> Result<Account, EnrollError> {
        let prepared = self.recall(id)?;
        let provider = prepared.observation.identity.provider();

        if let Err(why) = account::validate_slug(&draft.slug) {
            self.gateway.discard(&prepared.id);
            return Err(EnrollError::BadName(why));
        }
        if self.registry.exists(provider, &draft.slug) {
            self.gateway.discard(&prepared.id);
            return Err(EnrollError::Registry(RegistryError::AlreadyExists(format!(
                "{}/{}",
                provider.id(),
                draft.slug
            ))));
        }

        let account = self.account_for(provider, &draft, &prepared.observation);
        match self.registry.create(&account, &prepared.id) {
            Ok(()) => Ok(account),
            Err(e) => {
                self.gateway.discard(&prepared.id);
                Err(EnrollError::Registry(e))
            }
        }
    }

    /// 확인된 새 자격으로 계정의 자격을 교체한다.
    ///
    /// 다른 계정의 자격은 거부한다. 실패한 교체는 이력에 남지 않는다 — 이력은
    /// 실제로 쓰였던 자격의 기록이고, 붙지 못한 자격은 쓰인 적이 없다.
    pub fn reissue(&self, account: &Account, id: &PreparationId) -> Result<Account, EnrollError> {
        let prepared = self.recall(id)?;
        let observed = &prepared.observation;

        let mismatched = observed.identity.provider() != account.provider
            || same_account(&account.identity.name, observed.identity.name()).is_err();
        if mismatched {
            self.gateway.discard(&prepared.id);
            return Err(EnrollError::OtherAccount(format!(
                "다른 계정의 자격 증명입니다. 이 계정의 신원은 {}이며, 입력한 자격 증명의 신원은 {}입니다.",
                account.identity.name,
                observed.identity.name()
            )));
        }

        let record = Replacement {
            replaced_at: self.clock.now(),
            reason: ArchiveReason::Replaced,
            detail: match account.expiry_on(&self.clock.today()) {
                account::Expiry::Expired(_) => "만료되어 교체".to_string(),
                _ => "기한 전 교체".to_string(),
            },
            identity: account.identity.name.clone(),
            expires: account.expires.clone(),
            verified_at: account.verification.as_ref().map(|v| v.checked_at.clone()),
            scopes: account.scopes.clone(),
        };

        let mut updated = account.clone();
        self.apply_observation(&mut updated, observed);

        match self
            .registry
            .replace_credential(&updated, &prepared.id, record)
        {
            Ok(()) => Ok(updated),
            Err(e) => {
                self.gateway.discard(&prepared.id);
                Err(EnrollError::Registry(e))
            }
        }
    }

    /// 붙어 있는 자격으로 지금 누구인지 다시 묻고 결과를 남긴다.
    ///
    /// 자격이 거부당한 것과 결과를 기록하지 못한 것은 다른 일이다. 앞은 확인
    /// 결과로 남고, 뒤는 실패로 올라간다 — 기록되지 않은 확인은 한 적 없는 확인이다.
    pub fn recheck(
        &self,
        account: &Account,
        progress: &dyn ProgressSink,
    ) -> Result<Account, EnrollError> {
        let mut updated = account.clone();

        match self.gateway.identity(account, progress) {
            Ok(observed) => {
                self.apply_observation(&mut updated, &observed);
            }
            Err(e) => {
                updated.verification = Some(account::Verification {
                    checked_at: self.clock.now(),
                    ok: false,
                    detail: format!("신원을 확인하지 못했습니다: {e}"),
                });
            }
        }

        self.registry.save(&updated)?;
        Ok(updated)
    }

    /// 확인해 둔 자격을 다시 집어 든다. 확인한 적 없는 표로는 아무것도 할 수 없다.
    fn recall(&self, id: &PreparationId) -> Result<Prepared, EnrollError> {
        let observation = self
            .gateway
            .prepared(id)
            .ok_or(EnrollError::Registry(RegistryError::NothingPrepared))?;
        Ok(Prepared {
            id: id.clone(),
            observation,
        })
    }

    /// 관찰 결과와 사람이 적은 설명으로 계정을 만든다.
    fn account_for(
        &self,
        provider: Provider,
        draft: &Draft,
        observed: &Observation,
    ) -> Account {
        let mut account = Account::new(provider, &draft.slug);
        account.display = draft.display.clone();
        account.note = draft.note.clone();
        self.apply_observation(&mut account, observed);
        account
    }

    /// 관찰한 사실을 계정에 옮겨 적는다. 사실의 출처는 여기 하나뿐이다.
    fn apply_observation(&self, account: &mut Account, observed: &Observation) {
        account.identity.kind = observed.identity.kind().to_string();
        account.identity.name = observed.identity.name().to_string();
        account.git_email = observed.identity.git_email();
        account.aws_account_id = observed.identity.aws_account_id();
        account.scopes = observed.facts.scopes.clone();
        account.expires = observed.facts.expires.clone();
        account.root_keys_present = observed.facts.root_keys_present;
        account.root_mfa = observed.facts.root_mfa;
        account.verification = Some(account::Verification {
            checked_at: self.clock.now(),
            ok: true,
            detail: observed.identity.display(),
        });
    }
}
