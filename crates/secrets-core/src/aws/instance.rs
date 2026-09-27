//! 서버 기록이 생기기 전의 서버 계정 기록 — pem 아래 `instance/<인스턴스>/<계정>/key.toml`.
//!
//! 이제 계정의 정본은 서버 기록(`crate::server`)이다. 이 기록은 읽기만 하고, 등록하지 않은 서버를
//! 제안할 때 근거로 쓴다. 키 파일은 이 자리에 그대로 두고 서버 기록이 가리킨다.

use serde::{Deserialize, Serialize};

pub use crate::server::Role;

/// 어디까지 갔는가.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountState {
    Local,
    Installed,
    Verified,
}

/// 계정 하나의 기록. `key.toml` 의 모양.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceAccount {
    pub account: String,
    pub role: Role,
    #[serde(default)]
    pub purpose: String,

    pub instance: String,
    #[serde(default)]
    pub instance_name: String,
    pub address: String,
    /// 어느 pem 으로 심었나.
    pub keypair: String,
    pub region: String,
    /// pem 으로 들어갈 때 쓴 계정. EC2 우분투는 `ubuntu` 다.
    pub via: String,

    pub algorithm: String,
    pub fingerprint: String,
    pub workspace: String,
    pub group: String,

    pub created_at: String,
    pub state: AccountState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
    /// 이 도구가 계정까지 만들었는가.
    #[serde(default)]
    pub ours: bool,
}

impl InstanceAccount {
    pub fn slug(&self) -> String {
        format!("{}/{}", self.instance, self.account)
    }
}
