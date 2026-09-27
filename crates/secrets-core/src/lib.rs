//! 마스터 계정 도메인과, core 가 바깥에 요구하는 포트.
//!
//! 이 크레이트는 프로세스를 띄우지 않고 파일을 읽고 쓰지 않는다. 그런 일은 전부
//! 포트 뒤에 있고, 구현은 `secrets-local` 이 가진다. 의존성 그래프가 그것을 한 번,
//! crates/secrets-core/clippy.toml 의 금지 목록이 다시 한 번 강제한다.

pub mod account;
pub mod aws;
pub mod credential;
pub mod enrollment;
pub mod etc;
pub mod identity;
pub mod key;
pub mod port;
pub mod project;
pub mod server;
pub mod time;
