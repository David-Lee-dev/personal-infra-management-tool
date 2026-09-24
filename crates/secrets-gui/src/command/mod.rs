//! 화면이 부르는 명령.
//!
//! 각 명령은 배선에서 절차를 꺼내 부르고, 진행 상황을 이벤트로 흘린다.
//! 무엇을 어떤 순서로 할지는 절차 쪽이 정한다.

pub mod accounts;
pub mod aws;
pub mod hosts;
pub mod etc;
pub mod iam;
pub mod keys;
pub mod login;
pub mod switching;
pub mod tools;
