//! 서버에 들어가서 하는 일.
//!
//! ```text
//! script/  서버에서 돌 셸 조각
//! ssh/     pem 으로 들어가 그것을 돌리는 일
//! vault/   계정 키와 기록이 놓이는 곳
//! ```

pub mod script;
pub mod ssh;
pub mod terminal;
pub mod vault;

pub use ssh::SshHosts;
pub use vault::FileAccounts;
