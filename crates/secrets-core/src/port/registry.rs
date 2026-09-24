//! 계정 레지스트리.

use crate::account::{Account, Provider, Replacement};

use super::accounts::PreparationId;

/// 레지스트리를 건드리는 일이 실패한 이유.
#[derive(Debug)]
pub enum RegistryError {
    AlreadyExists(String),
    NotFound(String),
    /// 확인된 자격이 없다.
    NothingPrepared,
    /// 쓰지 못했다. 무엇을 되돌렸는지 함께 온다.
    Unwritable(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::AlreadyExists(what) => write!(f, "{what}이(가) 이미 있습니다."),
            RegistryError::NotFound(what) => write!(f, "{what}을(를) 찾을 수 없습니다."),
            RegistryError::NothingPrepared => {
                f.write_str("검증된 자격 증명이 없습니다. 먼저 자격 증명을 확인하세요.")
            }
            RegistryError::Unwritable(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for RegistryError {}

/// 계정 레지스트리.
///
/// `create` 와 `replace_credential` 은 **원자 단위**다. 성공하면 자격과 기록이
/// 모두 제자리에 있고, 실패하면 손대기 전 상태와 구별되지 않는다.
pub trait AccountRegistry: Send + Sync {
    fn exists(&self, provider: Provider, slug: &str) -> bool;
    fn load(&self, provider: Provider, slug: &str) -> Result<Account, RegistryError>;
    fn list(&self) -> Vec<Result<Account, String>>;

    /// 준비된 자격을 계정의 것으로 삼아 새 계정을 만든다.
    fn create(&self, account: &Account, prepared: &PreparationId) -> Result<(), RegistryError>;

    /// 준비된 자격으로 계정의 자격을 갈아 끼우고 교체 이력을 남긴다.
    fn replace_credential(
        &self,
        account: &Account,
        prepared: &PreparationId,
        record: Replacement,
    ) -> Result<(), RegistryError>;

    /// 계정 기록만 다시 쓴다. 자격은 건드리지 않는다.
    fn save(&self, account: &Account) -> Result<(), RegistryError>;
}

