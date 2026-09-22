//! 이 머신에 붙는 구현. core 가 선언한 포트를 실제 CLI·파일시스템·시계로 채운다.
//!
//! 프로세스 실행, `~/.secrets` 아래의 파일, 전역 설정 심링크가 전부 여기 있다.

pub mod active;
pub mod clock;
pub mod adapter;
pub mod connect;
pub mod exec;
pub mod home;
pub mod isolation;
pub mod paths;
pub mod store;
pub mod tools;
