//! core 가 바깥에 요구하는 것들. 구현은 바깥 계층이 가진다.
//!
//! 포트는 소비자가 소유하고 **도메인의 질문**을 드러낸다.

pub mod accounts;
pub mod clock;
pub mod progress;
pub mod registry;

pub use accounts::{AccountGateway, GatewayError, LoginChallenge, PreparationId, Prepared};
pub use clock::Clock;
pub use progress::{Channel, ProgressSink, Silent};
pub use registry::{AccountRegistry, RegistryError};
