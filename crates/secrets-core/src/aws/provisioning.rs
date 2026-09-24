//! 계정을 만들고 걷어내는 절차.
//!
//! 서버가 끼어들어 한 번에 묶이지 않으므로, 각 단계 사이에서 죽어도 **무엇이
//! 남았는지 화면이 말할 수 있는 자리**에만 멈춘다.
//!
//! - 키를 먼저 만들고 서버에 나중에 심는다. 반대로 하면 개인 키 없는 공개 키가
//!   서버에 남는다.
//! - 심은 뒤 **반드시 그 키로 들어가 본다.** 붙였다고 끝내면 못 들어가는 계정이
//!   `확인됨` 으로 남는다.

use crate::aws::instance::{
    AccountState, HostError, InstanceAccount, InstanceGateway, InstanceVault, Plan, Seat,
};
use crate::port::{Clock, ProgressSink};

pub struct Provisioning<'a> {
    gateway: &'a dyn InstanceGateway,
    vault: &'a dyn InstanceVault,
    clock: &'a dyn Clock,
}

impl<'a> Provisioning<'a> {
    pub fn new(
        gateway: &'a dyn InstanceGateway,
        vault: &'a dyn InstanceVault,
        clock: &'a dyn Clock,
    ) -> Provisioning<'a> {
        Provisioning {
            gateway,
            vault,
            clock,
        }
    }

    /// 이 인스턴스가 계정을 받을 준비가 되었는지 본다.
    pub fn inspect(
        &self,
        pem: &str,
        plan: &Plan,
        progress: &dyn ProgressSink,
    ) -> Result<crate::aws::instance::Readiness, HostError> {
        self.gateway.inspect(pem, plan, progress)
    }

    /// 모자란 것을 채운다.
    pub fn prepare(
        &self,
        pem: &str,
        plan: &Plan,
        progress: &dyn ProgressSink,
    ) -> Result<crate::aws::instance::Readiness, HostError> {
        self.gateway.prepare(pem, plan, progress)
    }

    pub fn list(&self) -> Vec<Result<InstanceAccount, String>> {
        self.vault.list()
    }

    pub fn load(&self, seat: &Seat) -> Result<InstanceAccount, HostError> {
        self.vault.load(seat)
    }

    /// 계정을 만든다.
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &self,
        pem: &str,
        seat: &Seat,
        plan: &Plan,
        purpose: &str,
        instance_name: &str,
        progress: &dyn ProgressSink,
    ) -> Result<InstanceAccount, HostError> {
        if self.vault.exists(seat) {
            return Err(HostError::Taken(seat.slug()));
        }

        // 없는 것을 모르고 심으면 반만 도는 계정이 남는다. 먼저 본다.
        let ready = self.gateway.inspect(pem, plan, progress)?;
        if !ready.ok() {
            return Err(HostError::NotReady(ready.missing()));
        }

        let comment = format!("secrets/{}", seat.slug());
        let (public_key, fingerprint, algorithm) = self.vault.create(seat, &comment)?;

        let mut record = InstanceAccount {
            account: seat.account.clone(),
            role: plan.role,
            purpose: purpose.to_string(),
            instance: seat.instance.clone(),
            instance_name: instance_name.to_string(),
            address: plan.address.clone(),
            keypair: seat.keypair.clone(),
            region: seat.region.clone(),
            via: plan.via.clone(),
            algorithm,
            fingerprint,
            workspace: plan.workspace.clone(),
            group: plan.group.clone(),
            created_at: self.clock.now(),
            state: AccountState::Local,
            verified_at: None,
            ours: false,
        };
        self.vault.record(&record)?;

        // 심기 전에 기록을 남겨 둔다. 여기서 죽으면 쓸 수 없는 키가 남지만
        // `서버에 없음` 으로 드러나고 다시 시도할 수 있다.
        let ours = match self.gateway.install(pem, seat, plan, &public_key, progress) {
            Ok(ours) => ours,
            Err(e) => {
                // 서버에 아무것도 남지 않았으므로 키도 남길 이유가 없다.
                self.vault.discard(seat);
                return Err(e);
            }
        };

        record.ours = ours;
        record.state = AccountState::Installed;
        self.vault.record(&record)?;

        self.confirm(record, seat, plan, progress)
    }

    /// 심은 계정을 그 키로 직접 열어 본다. 멈춘 자리에서 다시 부를 수 있다.
    pub fn confirm(
        &self,
        mut record: InstanceAccount,
        seat: &Seat,
        plan: &Plan,
        progress: &dyn ProgressSink,
    ) -> Result<InstanceAccount, HostError> {
        let key = self.vault.private_path(seat);
        self.gateway.verify(&key, seat, plan, progress)?;

        record.state = AccountState::Verified;
        record.verified_at = Some(self.clock.now());
        self.vault.record(&record)?;
        Ok(record)
    }

    /// 같은 키로 다시 심는다.
    ///
    /// 멈춘 자리에서 이어 갈 때도 쓰고, 서버 쪽 설정이 바뀌어 다시 걸어야 할 때도
    /// 쓴다. 스크립트는 몇 번 돌려도 같은 결과라 `확인됨` 인 계정에도 돌릴 수 있다 —
    /// 그럴 일이 없다고 막아 두면 정작 필요할 때 손쓸 방법이 없다.
    pub fn reinstall(
        &self,
        pem: &str,
        seat: &Seat,
        plan: &Plan,
        progress: &dyn ProgressSink,
    ) -> Result<InstanceAccount, HostError> {
        let ready = self.gateway.inspect(pem, plan, progress)?;
        if !ready.ok() {
            return Err(HostError::NotReady(ready.missing()));
        }

        let mut record = self.vault.load(seat)?;
        let public_key = self.vault.public_key(seat)?;
        let made = self.gateway.install(pem, seat, plan, &public_key, progress)?;
        // "우리가 만들었나" 는 처음 한 번 정해진다. 다시 심을 때는 계정이 이미
        // 있으니 거짓이 오는데, 그걸 그대로 적으면 우리가 만든 계정을 나중에
        // 지우지 못한다.
        record.ours = record.ours || made;
        record.state = AccountState::Installed;
        self.vault.record(&record)?;

        self.confirm(record, seat, plan, progress)
    }

    /// 계정을 걷어낸다. 서버에서 먼저 지우고 실물은 보관한다.
    ///
    /// 순서가 중요하다 — 기록을 먼저 치우면 무엇을 어디서 지워야 하는지 잃는다.
    pub fn remove(
        &self,
        pem: &str,
        seat: &Seat,
        progress: &dyn ProgressSink,
    ) -> Result<(), HostError> {
        let record = self.vault.load(seat)?;
        let plan = Plan {
            role: record.role,
            workspace: record.workspace.clone(),
            group: record.group.clone(),
            via: record.via.clone(),
            address: record.address.clone(),
        };

        self.gateway
            .remove(pem, seat, &plan, record.ours, progress)?;
        self.vault.archive(seat, "삭제")
    }
}
