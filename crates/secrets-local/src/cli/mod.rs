//! 외부 CLI 를 찾고 실행하는 일.
//!
//! 이 도구가 하는 일은 결국 CLI 를 대신 실행해 주는 것이다. 그 경로를 여기 하나로
//! 모아, 무엇을 실행했고 무엇이 나왔는지를 호출자가 전부 관찰할 수 있게 한다.

pub mod exec;
pub mod tools;
pub mod version;

pub use tools::find_in_path;
