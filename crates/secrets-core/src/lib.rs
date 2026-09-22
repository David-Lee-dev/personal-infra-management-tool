//! 번들 · 계정 · 툴 검사 로직. CLI 와 GUI 가 공유하는 유일한 로직 계층.
//! 비밀값은 이 크레이트 밖으로 나가지 않는다.

pub mod account;
pub mod adapter;
pub mod active;
pub mod connect;
pub mod credential;
pub mod date;
pub mod exec;
pub mod home;
pub mod identity;
pub mod isolation;
pub mod port;
pub mod registration;
pub mod secret;
pub mod tools;
pub mod version;

pub const ROOT_ENV: &str = "SECRETS_HOME";
pub const ROOT_DEFAULT: &str = "~/.secrets";
