//! 서버에 들어가서 하는 일.
//!
//! ```text
//! script/  서버에서 돌 셸 조각
//! ssh/     계정으로 들어가 그것을 돌리는 일
//! terminal/ 그 계정으로 들어가는 Ghostty 창
//! ```

pub mod script;
pub mod ssh;
pub mod terminal;

pub use ssh::SshHosts;
