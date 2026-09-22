//! `~/.secrets/accounts` 아래의 파일로 된 계정 레지스트리.
//!
//! 준비된 자격의 실물은 [`CredentialStore`] 가 쥐고 있고, 계정을 만들거나 자격을
//! 교체할 때 그 자리를 이 레지스트리가 넘겨받는다. 두 어댑터가 같은 보관소를
//! 공유하므로, core 는 경로를 한 번도 보지 않고 표만 주고받는다.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use secrets_core::account::{Account, Provider, Replacement};
use crate::vault;
use secrets_core::port::{AccountRegistry, PreparationId, RegistryError};

use super::accounts::CredentialStore;
use crate::vault::paths;
use crate::vault::store;

pub struct FileRegistry {
    store: Arc<CredentialStore>,
    /// 계정을 만들고 바꾸는 일을 한 번에 하나씩만 하게 한다.
    ///
    /// 두 등록이 겹치면 확인과 자리 잡기 사이에 서로를 덮어쓴다. 사람이 창 하나로
    /// 쓰는 도구에서 계정 등록이 동시에 일어날 이유가 없으므로 통째로 직렬화한다.
    writing: Mutex<()>,
}

impl FileRegistry {
    pub fn new(store: Arc<CredentialStore>) -> FileRegistry {
        FileRegistry {
            store,
            writing: Mutex::new(()),
        }
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
        let _claim = self.writing.lock().unwrap_or_else(|e| e.into_inner());

        // 호출자가 미리 본 `exists` 와 이 호출 사이에 다른 등록이 끼어들 수 있다.
        // 여기서 다시 보지 않으면 남의 자격을 조용히 덮어쓴다.
        if store::exists(account.provider, &account.slug) {
            return Err(RegistryError::AlreadyExists(format!(
                "{}/{}",
                account.provider.id(),
                account.slug
            )));
        }

        let stage = self
            .store
            .take(prepared)
            .ok_or(RegistryError::NothingPrepared)?;

        let placed = (|| -> io::Result<()> {
            let dir = paths::dir(account);
            vault::create_private(&dir)?;

            let cli = paths::cli_home(account);
            if cli.exists() {
                std::fs::remove_dir_all(&cli)?;
            }
            std::fs::rename(&stage, &cli)?;
            vault::restrict(&cli)?;
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
        let _claim = self.writing.lock().unwrap_or_else(|e| e.into_inner());

        let stage = self
            .store
            .take(prepared)
            .ok_or(RegistryError::NothingPrepared)?;

        let swap = match Swap::apply(account, &stage) {
            Ok(swap) => swap,
            Err(Broken { undo, cause }) => {
                undo();
                return Err(unwritable("새 자격을 끼우지 못했습니다", cause));
            }
        };

        // 이력은 교체가 실제로 끝난 뒤에만 쓴다. 붙지 못한 자격은 쓰인 적이 없다.
        let recorded = store::history_dir(account, record.replaced_at.split('T').next().unwrap_or(""));
        if let Err(e) = store::write_history(&recorded, &record) {
            // 쓰다 만 이력이 남으면 "실패한 교체는 이력에 없다" 가 거짓이 된다.
            let _ = std::fs::remove_dir_all(&recorded);
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

/// 갈아 끼우다 실패했다. 어디까지 갔든 되돌릴 수 있는 상태로 온다.
struct Broken {
    undo: Box<dyn FnOnce()>,
    cause: io::Error,
}

impl Swap {
    fn apply(account: &Account, stage: &Path) -> Result<Swap, Broken> {
        let live = paths::cli_home(account);
        let previous = paths::dir(account).join("cli.replaced");
        let _ = std::fs::remove_dir_all(&previous);

        let had_previous = live.exists();
        if had_previous && let Err(cause) = std::fs::rename(&live, &previous) {
            // 아직 아무것도 옮기지 못했다. 준비한 자격만 버린다.
            let stage = stage.to_path_buf();
            return Err(Broken {
                undo: Box::new(move || {
                    let _ = std::fs::remove_dir_all(&stage);
                }),
                cause,
            });
        }

        let swap = Swap {
            live,
            previous,
            had_previous,
        };

        if let Err(cause) = std::fs::rename(stage, &swap.live) {
            let stage = stage.to_path_buf();
            return Err(Broken {
                undo: Box::new(move || {
                    swap.undo();
                    let _ = std::fs::remove_dir_all(&stage);
                }),
                cause,
            });
        }

        // 권한을 조이지 못하면 남이 읽을 수 있는 자격이 제자리에 놓인다.
        // 반쪽 상태로 두지 않고 되돌린다.
        if let Err(cause) = vault::restrict(&swap.live) {
            return Err(Broken {
                undo: Box::new(move || swap.undo()),
                cause,
            });
        }

        Ok(swap)
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
