//! 등록 절차가 실패한 이유.

use crate::port::{GatewayError, RegistryError};

/// 등록 절차가 실패한 이유.
#[derive(Debug)]
pub enum EnrollError {
    Gateway(GatewayError),
    Registry(RegistryError),
    /// 슬러그 규칙에 맞지 않는다.
    BadName(String),
    /// 넣은 자격이 이 계정의 것이 아니다.
    OtherAccount(String),
    /// 자격의 필수 칸이 비었다.
    Missing(&'static str),
}

impl std::fmt::Display for EnrollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnrollError::Gateway(e) => write!(f, "{e}"),
            EnrollError::Registry(e) => write!(f, "{e}"),
            EnrollError::BadName(why) | EnrollError::OtherAccount(why) => f.write_str(why),
            EnrollError::Missing(field) => write!(f, "{field} 를 입력하세요"),
        }
    }
}

impl std::error::Error for EnrollError {}

impl From<GatewayError> for EnrollError {
    fn from(e: GatewayError) -> EnrollError {
        EnrollError::Gateway(e)
    }
}

impl From<RegistryError> for EnrollError {
    fn from(e: RegistryError) -> EnrollError {
        EnrollError::Registry(e)
    }
}

