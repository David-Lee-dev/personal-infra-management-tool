//! `~/.secrets/accounts` 아래의 파일로 된 계정 레지스트리.
//!
//! 준비된 자격의 실물은 [`CredentialStore`] 가 쥐고 있고, 계정을 만들거나 자격을
//! 교체할 때 그 자리를 이 레지스트리가 넘겨받는다. 두 어댑터가 같은 보관소를
//! 공유하므로, core 는 경로를 한 번도 보지 않고 표만 주고받는다.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use secrets_core::account::{Account, Provider, Replacement};
use crate::home;
use secrets_core::port::{AccountRegistry, PreparationId, RegistryError};

use super::cli_accounts::CredentialStore;
use crate::paths;
use crate::store;

pub struct FileRegistry {
    store: Arc<CredentialStore>,
}

impl FileRegistry {
    pub fn new(store: Arc<CredentialStore>) -> FileRegistry {
        FileRegistry { store }
    }
}

fn unwritable(what: &str, e: impl std::fmt::Display) -> RegistryError {
    RegistryError::Unwritable(format!("{what}: {e}"))
}

impl AccountRegistry for FileRegistry {
    fn exists(&self, provider: Provider, slug: &str) -> bool {
        store::exists(provider, slug)
    }

    fn load(&self, provider: Provider, slug: &str) -> Result<Account, RegistryError> {
        store::load(provider, slug)
            .map_err(|_| RegistryError::NotFound(format!("{}/{slug}", provider.id())))
    }

    fn list(&self) -> Vec<Result<Account, String>> {
        store::list()
    }

    /// 준비 홈을 계정의 CLI 홈 자리로 옮기고 **마지막에** 계정 기록을 쓴다.
    ///
    /// 레지스트리에 계정이 보인다는 것은 자격이 이미 제자리에 있다는 뜻이어야 한다.
    /// 중간에 실패하면 만들던 것을 지운다.
    fn create(&self, account: &Account, prepared: &PreparationId) -> Result<(), RegistryError> {
        let stage = self
            .store
            .take(prepared)
            .ok_or(RegistryError::NothingPrepared)?;

        let placed = (|| -> io::Result<()> {
            let dir = paths::dir(account);
            home::create_private(&dir)?;

            let cli = paths::cli_home(account);
            if cli.exists() {
                std::fs::remove_dir_all(&cli)?;
            }
            std::fs::rename(&stage, &cli)?;
            home::restrict(&cli)?;
            store::save(account)
        })();

        if let Err(e) = placed {
            let _ = std::fs::remove_dir_all(paths::dir(account));
            let _ = std::fs::remove_dir_all(&stage);
            return Err(unwritable("계정을 만들지 못했습니다", e));
        }
        Ok(())
    }

    /// 쓰던 자격을 옆으로 밀어 두고 새 자격을 끼운다.
    ///
    /// 이력이나 계정 기록을 쓰지 못하면 밀어 둔 자격을 제자리로 되돌린다.
    /// 실패한 교체는 이력에 남지 않는다.
    fn replace_credential(
        &self,
        account: &Account,
        prepared: &PreparationId,
        record: Replacement,
    ) -> Result<(), RegistryError> {
        let stage = self
            .store
            .take(prepared)
            .ok_or(RegistryError::NothingPrepared)?;

        let swap = Swap::apply(account, &stage)
            .map_err(|e| unwritable("새 자격을 끼우지 못했습니다", e))?;

        // 이력은 교체가 실제로 끝난 뒤에만 쓴다. 붙지 못한 자격은 쓰인 적이 없다.
        let recorded = store::history_dir(account, record.replaced_at.split('T').next().unwrap_or(""));
        if let Err(e) = store::write_history(&recorded, &record) {
            swap.undo();
            return Err(unwritable("교체 기록을 남기지 못해 교체를 되돌렸습니다", e));
        }

        if let Err(e) = store::save(account) {
            let _ = std::fs::remove_dir_all(&recorded);
            swap.undo();
            return Err(unwritable("교체한 자격을 기록하지 못해 되돌렸습니다", e));
        }

        swap.keep();
        Ok(())
    }

    fn save(&self, account: &Account) -> Result<(), RegistryError> {
        store::save(account).map_err(|e| unwritable("계정을 저장하지 못했습니다", e))
    }
}

/// CLI 홈을 새 것으로 갈아 끼운 상태. 되돌리거나 확정할 수 있다.
struct Swap {
    live: PathBuf,
    previous: PathBuf,
    had_previous: bool,
}

impl Swap {
    fn apply(account: &Account, stage: &Path) -> io::Result<Swap> {
        let live = paths::cli_home(account);
        let previous = paths::dir(account).join("cli.replaced");
        let _ = std::fs::remove_dir_all(&previous);

        let had_previous = live.exists();
        if had_previous {
            std::fs::rename(&live, &previous)?;
        }

        if let Err(e) = std::fs::rename(stage, &live) {
            if had_previous {
                let _ = std::fs::rename(&previous, &live);
            }
            let _ = std::fs::remove_dir_all(stage);
            return Err(e);
        }
        home::restrict(&live)?;

        Ok(Swap {
            live,
            previous,
            had_previous,
        })
    }

    fn undo(&self) {
        let _ = std::fs::remove_dir_all(&self.live);
        if self.had_previous {
            let _ = std::fs::rename(&self.previous, &self.live);
        }
    }

    fn keep(self) {
        let _ = std::fs::remove_dir_all(&self.previous);
    }
}
