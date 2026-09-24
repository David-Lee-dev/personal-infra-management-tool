//! AWS 자격 도메인.
//!
//! GitHub 배포 키와 달리 **두 가지 모양**이 섞여 있다.
//!
//! - **IAM 액세스 키** — `AKIA…` 와 시크릿. SSH 키 쌍이 아니고, AWS 가 시크릿을
//!   만들 때 한 번만 보여 준다. 그래서 기존 키를 금고로 가져올 방법이 없다.
//! - **pem 키** — 인스턴스에 붙는 키페어의 개인 키. SSH 키라 지문으로 맞춰 볼 수 있고,
//!   손에 있으면 그대로 들일 수 있다.
//!
//! 둘을 한 타입으로 묶지 않는다. 만드는 법도, 회전하는 법도, 가져올 수 있는지도
//! 다르기 때문이다.

pub mod fingerprint;
pub mod iam;
pub mod instance;
pub mod pairing;
pub mod provisioning;

use crate::port::ProgressSink;

pub use pairing::SeenKeyPair;

/// 키페어가 어느 서비스의 것인가. 조회하는 API 가 다르다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Machine {
    Ec2,
    Lightsail,
}

impl Machine {
    pub fn id(&self) -> &'static str {
        match self {
            Machine::Ec2 => "ec2",
            Machine::Lightsail => "lightsail",
        }
    }
}

/// 이 금고가 쥐고 있는 pem 키. AWS 는 개인 키를 다시 주지 않으므로 여기 있는 것이 유일본이다.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KeyPairRecord {
    pub name: String,
    /// AWS 계정 ID. 경로의 한 단계이자 이 키가 누구 것인지를 말한다.
    pub account: String,
    /// ec2 | lightsail
    pub machine: String,
    pub region: String,
    /// `ssh-keygen` 이 적는 형태로 보관한다. 우리가 늘 쓰는 쪽이다.
    pub fingerprint: String,
    /// 이 키가 무엇에 쓰이는가. 비어 있을 수 있다.
    #[serde(default)]
    pub purpose: String,
    /// AWS 가 말하는 지문과 맞춰 본 적이 있는가.
    ///
    /// AWS 가 지문을 주지 않는 것이 있다. 그때도 들이되,
    /// **확인하지 못했다는 사실을 기록에 남긴다.** 확인한 것과 같은 척하지 않는다.
    #[serde(default)]
    pub verified: bool,
    pub adopted_at: String,
}

impl KeyPairRecord {
    /// 화면과 기록에서 이 키를 가리키는 한 줄.
    pub fn slug(&self) -> String {
        format!("{}/{}/{}/{}", self.account, self.machine, self.region, self.name)
    }
}

#[derive(Debug)]
pub enum AwsError {
    /// AWS 가 거절했거나 닿지 못했다.
    Remote(String),
    /// 로컬 파일을 다루지 못했다.
    Storage(String),
    /// 손에 든 개인 키가 그 키페어의 것이 아니다.
    Mismatch { expected: String, found: String },
    /// 그 자리에 이미 키가 있다.
    Taken(String),
    /// 그 이름의 키페어도, 지문이 맞는 키페어도 없다.
    Absent { name: String, present: Vec<String> },
}

impl std::fmt::Display for AwsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AwsError::Remote(detail) => write!(f, "AWS: {detail}"),
            AwsError::Storage(detail) => write!(f, "{detail}"),
            AwsError::Mismatch { expected, found } => write!(
                f,
                "개인 키가 AWS 키페어와 일치하지 않습니다 (AWS: {expected}, 입력한 키: {found})."
            ),
            AwsError::Taken(at) => write!(f, "{at}에 키가 이미 있습니다."),
            AwsError::Absent { name, present } if present.is_empty() => {
                write!(f, "{name} 키페어가 없습니다. 이 리전에는 키페어가 하나도 없습니다")
            }
            AwsError::Absent { name, present } => write!(
                f,
                "{name} 키페어가 없고, 이 키와 지문이 맞는 키페어도 없습니다. 이 리전에 있는 것: {}",
                present.join(", ")
            ),
        }
    }
}

/// AWS 에 묻는 곳.
pub trait AwsGateway: Send + Sync {
    /// 이 계정의 AWS 계정 ID. 키가 앉을 자리의 첫 단계다.
    fn account_id(&self, account: &str, progress: &dyn ProgressSink) -> Result<String, AwsError>;

    /// 그 리전의 키페어 전부. Lightsail 은 기본 키페어도 포함한다.
    ///
    /// 이름 하나만 묻지 않는다. pem 파일 이름이 키페어 이름과 다를 수 있어 지문으로
    /// 찾아야 하고, 한 리전의 키페어는 몇 개뿐이다.
    fn key_pairs(
        &self,
        account: &str,
        machine: Machine,
        region: &str,
        progress: &dyn ProgressSink,
    ) -> Result<Vec<SeenKeyPair>, AwsError>;
}
