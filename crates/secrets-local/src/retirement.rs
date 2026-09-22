//! 계정을 현역에서 내린다.
//!
//! 전역으로 쓰이던 계정을 그냥 옮기면 링크가 끊어져 CLI 가 통째로 망가진다.
//! 그래서 걷어내는 일과 보관하는 일은 순서가 있고, 그 순서와 실패 처리는 한 곳에
//! 있어야 한다 — 화면이 정할 일이 아니다.

use std::io;
use std::path::PathBuf;

use secrets_core::account::{ArchiveReason, Provider};

use crate::vault::store;
use crate::switching;

/// 내린 결과.
#[derive(Debug)]
pub struct Retired {
    /// 전역 링크를 걷어냈는가.
    pub was_active: bool,
    /// 보관된 자리.
    pub archived: PathBuf,
}

/// 계정을 걷어내고 보관소로 옮긴다.
///
/// 보관에 실패하면 걷어냈던 것을 되돌린다. 계정이 남아 있는데 아무 데서도 쓰이지
/// 않는 상태로 두지 않는다.
pub fn retire(provider: Provider, slug: &str, reason: ArchiveReason) -> io::Result<Retired> {
    let account = store::load(provider, slug)?;

    let was_active = switching::is_active(&account);
    if was_active {
        switching::deactivate(provider)?;
    }

    match store::archive_account(provider, slug, reason) {
        Ok(archived) => Ok(Retired {
            was_active,
            archived,
        }),
        Err(e) => {
            if was_active {
                let _ = switching::activate(&account);
            }
            Err(e)
        }
    }
}
