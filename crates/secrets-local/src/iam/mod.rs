//! IAM 을 이 머신에서 다루는 구현.
//!
//! ```text
//! gateway/    마스터 계정으로 AWS IAM 에 하는 일
//! vault/      기록 · 정책 원문 · 시크릿이 놓이는 곳
//! ```
//!
//! 소비처의 `.env` 는 건드리지 않는다. 넣고 빼는 일은 사람이 한다.

pub mod gateway;
pub mod vault;

pub use gateway::CliIam;
pub use vault::FileIam;
