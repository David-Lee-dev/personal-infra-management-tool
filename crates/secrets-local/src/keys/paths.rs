//! `~/.secrets/keys` 안에서 키가 차지하는 자리.
//!
//! ```text
//! keys/github/repo/<owner>/<repo>/<name>/
//!   key.toml      기록
//!   key           개인 키  0600
//!   key.pub       공개 키
//!   staging/      만드는 중인 키 쌍. 제자리에 놓기 전의 대기 자리
//!   history/      재발급으로 물러난 키
//! ```
//!
//! 경로가 **어느 리포의 무슨 키인지**를 말한다. 그래서 키 자신은 역할을 갖지 않고,
//! 디렉토리를 훑는 것만으로 목록이 된다.

use std::path::PathBuf;

use secrets_core::key::KeyRef;

use crate::vault;

/// 키가 모이는 뿌리.
pub const KEYS: &str = "keys";
/// 기록 파일 이름.
pub const FILE: &str = "key.toml";
/// 개인 키 파일 이름. 알고리즘을 이름에 넣지 않는다 — 재발급으로 바뀔 수 있다.
pub const PRIVATE: &str = "key";
pub const PUBLIC: &str = "key.pub";
pub const STAGING: &str = "staging";
pub const HISTORY: &str = "history";

/// 도메인별 뿌리. 지금은 github 만 있다.
pub fn github_root() -> PathBuf {
    vault::root().join(KEYS).join("github").join("repo")
}

pub fn dir_of(at: &KeyRef) -> PathBuf {
    github_root()
        .join(at.repo.owner())
        .join(at.repo.name())
        .join(&at.purpose)
}

pub fn record_of(at: &KeyRef) -> PathBuf {
    dir_of(at).join(FILE)
}

pub fn private_of(at: &KeyRef) -> PathBuf {
    dir_of(at).join(PRIVATE)
}

pub fn public_of(at: &KeyRef) -> PathBuf {
    dir_of(at).join(PUBLIC)
}

pub fn staging_of(at: &KeyRef) -> PathBuf {
    dir_of(at).join(STAGING)
}

pub fn history_of(at: &KeyRef) -> PathBuf {
    dir_of(at).join(HISTORY)
}

/// 걷어낸 키가 가는 곳. 지우지 않고 옮긴다.
pub fn archive_of(at: &KeyRef, stamp: &str) -> PathBuf {
    vault::root()
        .join("archive")
        .join(KEYS)
        .join("github")
        .join("repo")
        .join(at.repo.owner())
        .join(at.repo.name())
        .join(format!("{}-{stamp}", at.purpose))
}
