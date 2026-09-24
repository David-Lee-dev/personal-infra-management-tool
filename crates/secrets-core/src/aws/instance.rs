//! 인스턴스 접속 계정.
//!
//! pem 으로 들어가 계정을 만들고, 그 계정 전용 키를 심고, **그 키로 직접 들어가
//! 확인한다.** GitHub 은 API 가 성공을 말해 주지만 여기는 들어가 봐야 안다.
//!
//! 계정은 두 가지뿐이다. 권한 종류를 늘리면 "제한 푸는 데 한 세월" 이 시작된다.
//! 정말 필요하면 사용자를 넓히는 게 아니라 관리자로 들어간다.

use serde::{Deserialize, Serialize};

use crate::port::ProgressSink;

/// 이 계정이 서버에서 무엇을 할 수 있는가.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// sudo 전부. 다른 계정의 홈에도 닿는다.
    Admin,
    /// sudo 없음. 자기 홈과 공용 자리만.
    User,
}

impl Role {
    pub fn id(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::User => "user",
        }
    }
}

/// 어디까지 갔는가.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountState {
    /// 키만 만들었다. 서버에는 아직 없다.
    Local,
    /// 서버에 심었다. 다만 들어가 보지는 않았다.
    Installed,
    /// 그 키로 실제로 들어가 봤다.
    Verified,
}

/// 계정 하나의 기록. `key.toml` 에 그대로 쓴다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceAccount {
    /// 서버의 로그인 이름. 이것이 곧 자리다.
    pub account: String,
    pub role: Role,
    /// 무엇에 쓰는 계정인가. 권한이 아니라 설명이다.
    #[serde(default)]
    pub purpose: String,

    pub instance: String,
    #[serde(default)]
    pub instance_name: String,
    pub address: String,
    /// 어느 pem 으로 심었나.
    pub keypair: String,
    pub region: String,
    /// pem 으로 들어갈 때 쓰는 계정. EC2 우분투는 `ubuntu` 다.
    pub via: String,

    pub algorithm: String,
    pub fingerprint: String,
    pub workspace: String,
    pub group: String,

    pub created_at: String,
    pub state: AccountState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
    /// 우리가 만든 계정인가. 아니면 지울 때 계정은 남긴다.
    #[serde(default)]
    pub ours: bool,
}

impl InstanceAccount {
    /// 화면과 기록에서 이 계정을 가리키는 한 줄.
    pub fn slug(&self) -> String {
        format!("{}/{}", self.instance, self.account)
    }
}

/// 계정이 앉을 자리.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seat {
    pub region: String,
    pub keypair: String,
    pub instance: String,
    pub account: String,
}

impl Seat {
    /// 리눅스 로그인 이름으로 쓸 수 있는 것만 받는다.
    ///
    /// 경로의 한 단계이기도 해서, 여기서 막지 않으면 엉뚱한 디렉토리를 만든다.
    pub fn new(region: &str, keypair: &str, instance: &str, account: &str) -> Option<Seat> {
        let account = account.trim();
        let valid = !account.is_empty()
            && account.len() <= 32
            && account.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
            && account
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'));

        if !valid || region.trim().is_empty() || keypair.trim().is_empty() || instance.trim().is_empty() {
            return None;
        }
        Some(Seat {
            region: region.trim().to_string(),
            keypair: keypair.trim().to_string(),
            instance: instance.trim().to_string(),
            account: account.to_string(),
        })
    }

    pub fn slug(&self) -> String {
        format!("{}/{}", self.instance, self.account)
    }
}

/// 계정을 만들 때 서버에 심을 것.
#[derive(Debug, Clone)]
pub struct Plan {
    pub role: Role,
    /// 공용 자리. 모든 계정이 함께 쓴다.
    pub workspace: String,
    pub group: String,
    /// pem 으로 들어갈 때 쓰는 계정.
    pub via: String,
    pub address: String,
}

/// 인스턴스가 계정을 받을 준비가 되었는가.
///
/// 만들기 전에 본다. 없는 것을 모르고 심으면 **반만 도는 계정**이 남고, 그건
/// 나중에 알아차리기 어렵다.
#[derive(Debug, Clone)]
pub struct Readiness {
    /// pem 으로 들어가 sudo 까지 쓸 수 있는가.
    pub sudo: bool,
    /// `setfacl` 이 있는가. 없으면 공용 자리 쓰기 공유가 umask 에 좌우된다.
    pub acl: bool,
    /// 계정을 만들 수 있는가.
    pub useradd: bool,
    /// sudo 규칙을 검사할 수 있는가. 이것 없이 규칙을 넣으면 안 된다.
    pub visudo: bool,
    /// 패키지를 깔 때 쓸 도구. 모르면 `None`.
    pub packager: Option<String>,
}

impl Readiness {
    /// 지금 계정을 만들 수 있는가.
    pub fn ok(&self) -> bool {
        self.missing().is_empty()
    }

    /// 무엇이 없는가. 화면이 그대로 보여 준다.
    pub fn missing(&self) -> Vec<&'static str> {
        let mut gaps = Vec::new();
        if !self.sudo {
            gaps.push("sudo");
        }
        if !self.useradd {
            gaps.push("useradd");
        }
        if !self.visudo {
            gaps.push("visudo");
        }
        if !self.acl {
            gaps.push("setfacl");
        }
        gaps
    }
}

#[derive(Debug)]
pub enum HostError {
    /// 그 자리에 이미 계정이 있다.
    Taken(String),
    Missing(String),
    /// 서버에 닿지 못했거나 서버가 거절했다.
    Remote(String),
    /// 심기는 했는데 그 키로 들어가지 못했다.
    Unreachable(String),
    /// 인스턴스가 아직 계정을 받을 준비가 안 됐다.
    NotReady(Vec<&'static str>),
    Storage(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::Taken(at) => write!(f, "{at} 자리에 이미 계정이 있습니다"),
            HostError::Missing(at) => write!(f, "{at} 자리에 계정이 없습니다"),
            HostError::Remote(detail) => write!(f, "서버: {detail}"),
            HostError::Unreachable(detail) => {
                write!(f, "심었지만 그 키로 들어가지 못했습니다: {detail}")
            }
            HostError::NotReady(missing) => write!(
                f,
                "이 인스턴스에 {} 이(가) 없습니다. 준비를 먼저 하세요",
                missing.join(" · ")
            ),
            HostError::Storage(detail) => write!(f, "{detail}"),
        }
    }
}

/// 서버에 들어가서 하는 일.
pub trait InstanceGateway: Send + Sync {
    /// 이 인스턴스가 계정을 받을 준비가 되었는지 본다. 아무것도 바꾸지 않는다.
    fn inspect(&self, pem: &str, plan: &Plan, progress: &dyn ProgressSink)
    -> Result<Readiness, HostError>;

    /// 모자란 것을 채운다. 지금은 `acl` 하나다.
    fn prepare(&self, pem: &str, plan: &Plan, progress: &dyn ProgressSink)
    -> Result<Readiness, HostError>;

    /// 계정·공용 자리·키·권한을 심는다. 다시 불러도 같은 결과여야 한다.
    ///
    /// 돌려주는 값은 **우리가 그 계정을 만들었는가**다. 이미 있던 계정이면 거짓이고,
    /// 지울 때 계정은 남긴다.
    fn install(
        &self,
        pem: &str,
        seat: &Seat,
        plan: &Plan,
        public_key: &str,
        progress: &dyn ProgressSink,
    ) -> Result<bool, HostError>;

    /// 그 키로 실제로 들어가 본다. 관리자면 sudo 도 확인한다.
    fn verify(
        &self,
        private_key: &str,
        seat: &Seat,
        plan: &Plan,
        progress: &dyn ProgressSink,
    ) -> Result<(), HostError>;

    /// 권한·키·계정을 걷어낸다. `ours` 가 거짓이면 계정은 남긴다.
    fn remove(
        &self,
        pem: &str,
        seat: &Seat,
        plan: &Plan,
        ours: bool,
        progress: &dyn ProgressSink,
    ) -> Result<(), HostError>;
}

/// 계정의 키와 기록이 놓이는 곳.
pub trait InstanceVault: Send + Sync {
    fn exists(&self, seat: &Seat) -> bool;
    /// 키 쌍을 만들고 제자리에 놓는다. 돌려주는 것은 (공개 키, 지문, 알고리즘).
    fn create(&self, seat: &Seat, comment: &str) -> Result<(String, String, String), HostError>;
    fn public_key(&self, seat: &Seat) -> Result<String, HostError>;
    /// 개인 키 파일의 자리. ssh 에 넘길 값이라 경로 그대로 준다.
    fn private_path(&self, seat: &Seat) -> String;
    fn record(&self, account: &InstanceAccount) -> Result<(), HostError>;
    fn load(&self, seat: &Seat) -> Result<InstanceAccount, HostError>;
    fn list(&self) -> Vec<Result<InstanceAccount, String>>;
    fn archive(&self, seat: &Seat, reason: &str) -> Result<(), HostError>;
    fn discard(&self, seat: &Seat);
}
