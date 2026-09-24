//! 이 머신에 붙는 구현. core 가 선언한 포트를 실제 CLI·파일시스템·시계로 채운다.
//!
//! ```text
//! vault/     ~/.secrets 라는 저장소 — 뿌리·경로·계정 기록
//! cli/       외부 CLI 를 찾고 실행하는 일
//! provider/  provider 별 CLI 프로토콜
//! adapter/   위의 것들로 core 의 포트를 채운 구현
//! ```

pub mod adapter;
pub mod aws;
pub mod aws_vault;
pub mod cli;
pub mod clock;
pub mod etc;
pub mod hosts;
pub mod iam;
pub mod isolation;
pub mod keys;
pub mod provider;
pub mod retirement;
pub mod switching;
pub mod vault;
