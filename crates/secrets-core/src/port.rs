//! core 가 바깥에 요구하는 것들. 구현은 바깥 계층이 가진다.
//!
//! 포트는 소비자가 소유하고 **도메인의 질문**을 드러낸다. "명령을 실행해 달라"가
//! 아니라 "이 자격이 누구인지 확인해 달라"로 적는다. 어떤 CLI 를 어떤 인자로
//! 부르는지는 core 의 관심사가 아니다.

use crate::account::{Account, Provider, Replacement};
use crate::credential::CredentialInput;
use crate::identity::Observation;
use crate::secret::Secret;

/// 진행 상황이 나가는 줄. 사람이 보고 있는 창에 그대로 흐른다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Out,
    Err,
}

/// 오래 걸리는 일이 진행 중임을 알리는 곳.
pub trait ProgressSink: Send + Sync {
    fn line(&self, channel: Channel, text: &str);
}

/// 아무 데도 보내지 않는다. 조용히 돌려야 하는 자리와 테스트에 쓴다.
pub struct Silent;

impl ProgressSink for Silent {
    fn line(&self, _channel: Channel, _text: &str) {}
}

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
                "이 로그인 세션은 이미 끝났습니다. 코드를 한 번 잘못 넣으면 세션이 소멸하므로 다시 시작해 새 주소와 코드를 받으세요",
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

/// 레지스트리를 건드리는 일이 실패한 이유.
#[derive(Debug)]
pub enum RegistryError {
    AlreadyExists(String),
    NotFound(String),
    /// 확인된 자격이 없다.
    NothingPrepared,
    /// 쓰지 못했다. 무엇을 되돌렸는지 함께 온다.
    Unwritable(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::AlreadyExists(what) => write!(f, "{what} 는 이미 있습니다"),
            RegistryError::NotFound(what) => write!(f, "{what} 를 찾을 수 없습니다"),
            RegistryError::NothingPrepared => {
                f.write_str("확인된 자격이 없습니다. 자격 확인을 먼저 하세요")
            }
            RegistryError::Unwritable(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for RegistryError {}

/// 계정 레지스트리.
///
/// `create` 와 `replace_credential` 은 **원자 단위**다. 성공하면 자격과 기록이
/// 모두 제자리에 있고, 실패하면 손대기 전 상태와 구별되지 않는다.
pub trait AccountRegistry: Send + Sync {
    fn exists(&self, provider: Provider, slug: &str) -> bool;
    fn load(&self, provider: Provider, slug: &str) -> Result<Account, RegistryError>;
    fn list(&self) -> Vec<Result<Account, String>>;

    /// 준비된 자격을 계정의 것으로 삼아 새 계정을 만든다.
    fn create(&self, account: &Account, prepared: &PreparationId) -> Result<(), RegistryError>;

    /// 준비된 자격으로 계정의 자격을 갈아 끼우고 교체 이력을 남긴다.
    fn replace_credential(
        &self,
        account: &Account,
        prepared: &PreparationId,
        record: Replacement,
    ) -> Result<(), RegistryError>;

    /// 계정 기록만 다시 쓴다. 자격은 건드리지 않는다.
    fn save(&self, account: &Account) -> Result<(), RegistryError>;
}

/// 지금이 언제인가. 만료 판정과 기록 시각이 여기서 온다.
pub trait Clock: Send + Sync {
    /// `YYYY-MM-DD HH:MM` 형태의 지금.
    fn now(&self) -> String;
    /// `YYYY-MM-DD` 형태의 오늘.
    fn today(&self) -> String;
}
