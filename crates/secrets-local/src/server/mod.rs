//! 서버 기록과 계정 키가 놓이는 곳, 그리고 등록하지 않은 서버를 찾는 근거.
//!
//! ```text
//! ~/.secrets/servers/<id>.toml             서버 한 대 — 속성과 계정 목록
//! ~/.secrets/keys/server/<id>/<계정>/key   이 도구가 새로 만든 계정 키
//! ~/.secrets/archive/servers/              등록 해제한 서버 기록
//! ```

pub mod keys;
pub mod legacy;
pub mod seats;
pub mod store;

pub use keys::VaultKeys;
pub use seats::RegisteredSeats;
pub use store::FileServers;
