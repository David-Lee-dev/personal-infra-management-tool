//! 이 머신에 붙는 배선. 어떤 구현을 쓸지는 여기서만 고른다.

use secrets_core::enrollment::Enrollment;
use secrets_local::adapter::{accounts::CliAccounts, clock::SystemClock, registry::FileRegistry};

pub struct Wiring {
    gateway: CliAccounts,
    registry: FileRegistry,
    pub clock: SystemClock,
}

impl Wiring {
    pub fn get() -> &'static Wiring {
        static WIRING: std::sync::OnceLock<Wiring> = std::sync::OnceLock::new();
        WIRING.get_or_init(|| {
            // 게이트웨이와 레지스트리가 같은 보관소를 공유한다. 확인된 자격의
            // 자리를 그대로 계정에게 넘기기 위한 것이다.
            let store = std::sync::Arc::new(
                secrets_local::adapter::accounts::CredentialStore::new(),
            );
            Wiring {
                gateway: CliAccounts::new(store.clone()),
                registry: FileRegistry::new(store),
                clock: SystemClock,
            }
        })
    }

    pub fn enrollment(&self) -> Enrollment<'_> {
        Enrollment::new(&self.gateway, &self.registry, &self.clock)
    }
}
