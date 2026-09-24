//! 이 머신에 붙는 배선. 어떤 구현을 쓸지는 여기서만 고른다.

use secrets_core::aws::AwsGateway;
use secrets_core::aws::iam::Issuer;
use secrets_core::aws::instance::InstanceVault;
use secrets_core::aws::provisioning::Provisioning;
use secrets_core::enrollment::Enrollment;
use secrets_core::etc::EtcBook;
use secrets_core::key::Keyring;
use secrets_local::adapter::{accounts::CliAccounts, clock::SystemClock, registry::FileRegistry};
use secrets_local::aws::CliAws;
use secrets_local::etc::FileEtc;
use secrets_local::hosts::SshHosts;
use secrets_local::iam::{CliIam, FileIam};
use secrets_local::keys::{FileKeys, GhKeys};

pub struct Wiring {
    gateway: CliAccounts,
    registry: FileRegistry,
    keys: GhKeys,
    cloud: CliAws,
    remote: SshHosts,
    vault: FileKeys,
    iam: CliIam,
    iam_vault: FileIam,
    etc: FileEtc,
    pub clock: SystemClock,
}

impl Wiring {
    pub fn get() -> &'static Wiring {
        static WIRING: std::sync::OnceLock<Wiring> = std::sync::OnceLock::new();
        WIRING.get_or_init(|| {
            // 게이트웨이와 레지스트리가 같은 보관소를 공유한다. 확인된 자격의
            // 자리를 그대로 계정에게 넘기기 위한 것이다.
            let store = std::sync::Arc::new(
                secrets_local::adapter::PreparationStore::new(),
            );
            Wiring {
                gateway: CliAccounts::new(store.clone()),
                registry: FileRegistry::new(store),
                keys: GhKeys,
                cloud: CliAws,
                remote: SshHosts,
                vault: FileKeys,
                iam: CliIam,
                iam_vault: FileIam,
                etc: FileEtc,
                clock: SystemClock,
            }
        })
    }

    pub fn enrollment(&self) -> Enrollment<'_> {
        Enrollment::new(&self.gateway, &self.registry, &self.clock)
    }

    pub fn aws(&self) -> &dyn AwsGateway {
        &self.cloud
    }

    /// 금고가 어느 AWS 계정을 보는지는 호출자가 정한다. 그래서 값으로 받는다.
    pub fn provisioning<'a>(&'a self, vault: &'a dyn InstanceVault) -> Provisioning<'a> {
        Provisioning::new(&self.remote, vault, &self.clock)
    }

    pub fn issuer(&self) -> Issuer<'_> {
        Issuer::new(&self.iam, &self.iam_vault, &self.clock)
    }

    pub fn etc_book(&self) -> EtcBook<'_> {
        EtcBook::new(&self.etc, &self.clock)
    }

    pub fn etc_vault(&self) -> &FileEtc {
        &self.etc
    }

    pub fn keyring(&self) -> Keyring<'_> {
        Keyring::new(&self.keys, &self.vault, &self.clock)
    }
}
