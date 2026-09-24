//! 마스터 계정 provider 에게 묻는 질문.
//!
//! "이 명령을 이 환경변수로 실행해 달라" 가 아니라 "이 자격이 누구인지 확인해 달라"
//! 로 적는다. 어떤 CLI 를 어떤 인자로 부르는지는 core 의 관심사가 아니다.

use crate::account::{Account, Provider};
use crate::credential::CredentialInput;
use crate::credential::secret::Secret;
use crate::identity::Observation;

use super::progress::ProgressSink;

/// 확인이 끝나 붙이기만 남은 자격을 가리키는 표.
///
/// 자격의 실물은 이 표 뒤에 있고 core 로 넘어오지 않는다.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PreparationId(String);

impl PreparationId {
    pub fn named(text: impl Into<String>) -> PreparationId {
        PreparationId(text.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 확인이 끝난 자격과, 그때 본 신원.
#[derive(Debug)]
pub struct Prepared {
    pub id: PreparationId,
    pub observation: Observation,
}

/// 사람이 브라우저에서 해야 할 일.
#[derive(Debug, Clone)]
pub struct LoginChallenge {
    pub url: String,
    /// 브라우저 페이지에서 눈으로 대조할 값. 입력할 값이 아니다.
    pub session: String,
    pub note: String,
}

/// provider 에게 자격을 묻는 일이 실패한 이유.
#[derive(Debug)]
pub enum GatewayError {
    /// 자격이 거부당했다.
    Rejected(String),
    /// 마스터 계정으로 쓰기에 권한이 모자란다.
    NotPermitted(String),
    /// 그 로그인은 더 이상 쓸 수 없다.
    SessionGone,
    /// 도구가 없거나 실행하지 못했다.
    Unavailable(String),
}

impl std::fmt::Display for GatewayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GatewayError::Rejected(why) | GatewayError::NotPermitted(why) => f.write_str(why),
            GatewayError::SessionGone => f.write_str(
                "이 로그인 세션은 이미 종료되었습니다. 코드를 잘못 입력하면 세션이 종료되므로 로그인을 다시 시작해 새 주소와 코드를 받으세요.",
            ),
            GatewayError::Unavailable(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for GatewayError {}

/// 마스터 계정 provider 에게 묻는 질문.
pub trait AccountGateway: Send + Sync {
    /// 받아 적은 자격으로 로그인하고 누구인지 확인한다.
    fn prepare(
        &self,
        provider: Provider,
        credential: CredentialInput,
        progress: &dyn ProgressSink,
    ) -> Result<Prepared, GatewayError>;

    /// 브라우저로 한 번에 끝나는 로그인.
    fn prepare_with_browser(
        &self,
        provider: Provider,
        progress: &dyn ProgressSink,
    ) -> Result<Prepared, GatewayError>;

    /// 코드를 받아 와야 끝나는 로그인의 첫 단계.
    fn begin_browser_login(
        &self,
        provider: Provider,
        progress: &dyn ProgressSink,
    ) -> Result<(PreparationId, LoginChallenge), GatewayError>;

    /// 받아 온 코드로 같은 로그인을 끝낸다.
    fn complete_browser_login(
        &self,
        id: &PreparationId,
        code: &Secret,
        progress: &dyn ProgressSink,
    ) -> Result<Prepared, GatewayError>;

    /// 이미 붙어 있는 자격으로 지금 누구인지 다시 묻는다.
    fn identity(
        &self,
        account: &Account,
        progress: &dyn ProgressSink,
    ) -> Result<Observation, GatewayError>;

    /// 확인해 둔 자격의 관찰 결과. 화면을 한 번 다녀온 뒤에도 사실은 여기서 온다.
    fn prepared(&self, id: &PreparationId) -> Option<Observation>;

    /// 쓰지 않기로 한 자격을 버린다.
    fn discard(&self, id: &PreparationId);
}

