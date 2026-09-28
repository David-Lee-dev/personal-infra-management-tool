//! 서버 — 접속할 수 있는 기계 하나와 그 위의 계정들.
//!
//! 어디에 있는 기계인지(AWS EC2 · Lightsail · 그 밖)는 보여 주는 속성일 뿐이다. 접속 · 계정
//! 만들기 · 배포는 종류와 무관하게 **주소 + 계정 + 키**로만 한다. 주소는 서버 기록 한 곳에만
//! 있고, 계정과 프로젝트 환경은 서버를 가리키기만 한다.
//!
//! ```text
//! servers/<id>.toml      서버 한 대 — 속성과 계정 목록. 키 값은 없다
//! ```

pub mod provision;
pub mod registry;
pub mod suggest;

use serde::{Deserialize, Serialize};

pub use provision::AccountProvisioning;
pub use registry::{AccountChoice, NewAccount, NewServer, ServerEdit, Servers};
pub use suggest::{ConfigHost, LegacyAccount, Suggestion};

/// 이 계정이 서버에서 무엇을 할 수 있는가.
///
/// 계정은 두 가지뿐이다. 권한 종류를 늘리면 제한을 푸는 데 시간이 든다. 정말 필요하면
/// 사용자를 넓히는 게 아니라 관리자로 들어간다.
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

/// 서버가 어디에 있는가. 보여 주는 사실이다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerKind {
    Ec2,
    Lightsail,
    /// 로컬 기기 · 다른 클라우드 등 AWS 밖.
    Other,
}

impl ServerKind {
    pub fn id(&self) -> &'static str {
        match self {
            ServerKind::Ec2 => "ec2",
            ServerKind::Lightsail => "lightsail",
            ServerKind::Other => "other",
        }
    }

    pub fn parse(text: &str) -> Option<ServerKind> {
        match text {
            "ec2" => Some(ServerKind::Ec2),
            "lightsail" => Some(ServerKind::Lightsail),
            "other" => Some(ServerKind::Other),
            _ => None,
        }
    }

    /// AWS 서버인가. pem 키는 AWS 서버에서만 쓸 수 있다.
    pub fn is_aws(&self) -> bool {
        !matches!(self, ServerKind::Other)
    }
}

/// AWS 서버의 사실. pem 의 자리를 찾는 데도 쓴다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AwsFacts {
    pub account: String,
    pub region: String,
    /// 인스턴스 ID. 모르면 비어 있다.
    #[serde(default)]
    pub instance: String,
}

/// 계정으로 들어갈 때 쓰는 키가 어디 있는가.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AccountKey {
    /// 시크릿 저장소 안의 키 파일. 저장소 뿌리 기준 상대 경로.
    Vault { path: String },
    /// 서버의 AWS 계정 · 리전에 있는 그 키 페어의 pem.
    Pem { keypair: String },
    /// 로컬의 키 파일. 가리키기만 하고 옮기지 않는다.
    File { path: String },
    /// 지정하지 않는다 — ssh-agent · `~/.ssh/id_*`.
    Agent,
}

impl AccountKey {
    pub fn id(&self) -> &'static str {
        match self {
            AccountKey::Vault { .. } => "vault",
            AccountKey::Pem { .. } => "pem",
            AccountKey::File { .. } => "file",
            AccountKey::Agent => "agent",
        }
    }
}

/// 이 계정이 어떻게 이 도구의 기록이 되었는가. 무엇을 서버에서 걷어낼 수 있는지가 여기서 갈린다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountOrigin {
    /// 이 도구가 계정을 만들고 키를 심었다. 서버에서 제거하면 계정도 지운다.
    Created,
    /// 원래 있던 계정에 이 도구가 키를 심었다. 서버에서 제거하면 키 · 권한만 걷는다.
    Installed,
    /// 원래 있던 계정을 기록만 했다. 서버에는 손대지 않는다.
    Registered,
}

impl AccountOrigin {
    /// 이 도구가 서버에 키를 심은 계정인가 — 다시 심기 · 서버에서 제거를 할 수 있다.
    pub fn managed(&self) -> bool {
        !matches!(self, AccountOrigin::Registered)
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
    /// 기록만 했고 아직 들어가 보지 않았거나, 들어가지 못했다.
    Unverified,
}

impl AccountState {
    pub fn id(&self) -> &'static str {
        match self {
            AccountState::Local => "local",
            AccountState::Installed => "installed",
            AccountState::Verified => "verified",
            AccountState::Unverified => "unverified",
        }
    }
}

/// 서버의 계정 하나.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerAccount {
    pub login: String,
    pub role: Role,
    /// 무엇에 쓰는 계정인가. 권한이 아니라 설명이다.
    #[serde(default)]
    pub purpose: String,
    pub key: AccountKey,
    pub origin: AccountOrigin,
    pub state: AccountState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
    /// 이 도구가 만든 키의 지문.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub fingerprint: String,
}

fn default_port() -> u16 {
    22
}

fn default_workspace() -> String {
    "/srv".into()
}

fn default_workspace_group() -> String {
    "workspace".into()
}

/// 서버 한 대. `servers/<id>.toml` 의 모양.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Server {
    /// 등록할 때 정해지고 바뀌지 않는다. 프로젝트 환경이 이것으로 서버를 가리킨다.
    pub id: String,
    /// 사람이 정하고 바꿀 수 있는 이름.
    pub name: String,
    /// 프로젝트 그룹과 같은 목록에서 고른다. 없으면 비어 있다.
    #[serde(default)]
    pub group: String,
    /// IP 또는 DNS.
    pub address: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub kind: ServerKind,
    /// 관리 접속 — 계정을 만들 때 들어가는 sudo 계정의 로그인.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin: Option<String>,
    /// 새 계정이 함께 쓰는 공용 작업 디렉터리와 그 그룹.
    #[serde(default = "default_workspace")]
    pub workspace: String,
    #[serde(default = "default_workspace_group")]
    pub workspace_group: String,
    #[serde(default)]
    pub note: String,
    pub registered_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aws: Option<AwsFacts>,
    #[serde(default)]
    pub accounts: Vec<ServerAccount>,
}

impl Server {
    pub fn account(&self, login: &str) -> Option<&ServerAccount> {
        self.accounts.iter().find(|a| a.login == login)
    }

    fn account_mut(&mut self, login: &str) -> Option<&mut ServerAccount> {
        self.accounts.iter_mut().find(|a| a.login == login)
    }

    /// 화면과 작업 로그에서 이 서버의 계정을 가리키는 한 줄.
    pub fn slug(&self, login: &str) -> String {
        format!("{}/{login}", self.name)
    }
}

/// 한 계정으로 서버에 들어가는 데 필요한 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Access {
    /// 개인 키 파일의 절대 경로. 없으면 ssh 기본 키에 맡긴다.
    pub key: Option<String>,
    pub login: String,
    pub address: String,
    pub port: u16,
}

/// 서버에 키를 심는 이유. 새 계정은 서버에 같은 로그인이 있으면 거부한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallMode {
    New,
    Reinstall,
}

/// 계정을 만들거나 기존 키를 다시 심을 때 서버에 보낼 것.
#[derive(Debug, Clone)]
pub struct Install {
    pub mode: InstallMode,
    pub login: String,
    pub role: Role,
    pub workspace: String,
    pub group: String,
    pub public_key: String,
}

/// 서버가 계정을 받을 준비가 되었는가.
///
/// 만들기 전에 본다. 없는 것을 모르고 심으면 반만 도는 계정이 남고, 그건 나중에 알아차리기 어렵다.
#[derive(Debug, Clone)]
pub struct Readiness {
    /// 관리 접속으로 sudo 까지 쓸 수 있는가.
    pub sudo: bool,
    /// `setfacl` 이 있는가. 없으면 공용 자리 쓰기 공유가 umask 에 좌우된다.
    pub acl: bool,
    pub useradd: bool,
    /// sudo 규칙을 검사할 수 있는가. 이것 없이 규칙을 넣으면 안 된다.
    pub visudo: bool,
    /// 패키지를 깔 때 쓸 도구. 모르면 `None`.
    pub packager: Option<String>,
}

impl Readiness {
    pub fn ok(&self) -> bool {
        self.missing().is_empty()
    }

    /// 무엇이 없는가. 화면이 그대로 보여 준다.
    pub fn missing(&self) -> Vec<&'static str> {
        [
            (self.sudo, "sudo"),
            (self.useradd, "useradd"),
            (self.visudo, "visudo"),
            (self.acl, "setfacl"),
        ]
        .into_iter()
        .filter(|(present, _)| !present)
        .map(|(_, name)| name)
        .collect()
    }
}

#[derive(Debug)]
pub enum ServerError {
    /// 사람이 적은 값이 쓸 수 없는 모양이다.
    Invalid(String),
    Missing(String),
    /// 같은 이름 · 주소 · 계정이 이미 있다.
    Taken(String),
    /// 서버에 닿지 못했거나 서버가 거절했다.
    Remote(String),
    /// 심기는 했는데 그 키로 들어가지 못했다.
    Unreachable(String),
    /// 서버가 아직 계정을 받을 준비가 안 됐다.
    NotReady(Vec<&'static str>),
    Storage(String),
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerError::Invalid(detail) | ServerError::Storage(detail) => write!(f, "{detail}"),
            ServerError::Missing(what) => write!(f, "{what}을(를) 찾을 수 없습니다."),
            ServerError::Taken(what) => write!(f, "{what}은(는) 이미 있습니다."),
            ServerError::Remote(detail) => write!(f, "서버: {detail}"),
            ServerError::Unreachable(detail) => write!(f, "접속하지 못했습니다: {detail}"),
            ServerError::NotReady(missing) => write!(
                f,
                "이 서버에 {} 항목이 없습니다. 먼저 준비하세요.",
                missing.join(" · ")
            ),
        }
    }
}

/// 서버 기록이 놓이는 곳.
pub trait ServerStore: Send + Sync {
    /// 전부. 읽지 못한 기록은 건너뛰지 않고 오류로 돌려준다.
    fn list(&self) -> Vec<Result<Server, String>>;
    fn load(&self, id: &str) -> Result<Server, ServerError>;
    /// 새 기록을 쓴다. 같은 id 가 있으면 `Taken` 이다.
    fn insert(&self, server: &Server) -> Result<(), ServerError>;
    /// 있는 기록을 통째로 다시 쓴다. 없으면 `Missing` 이다.
    fn replace(&self, server: &Server) -> Result<(), ServerError>;
    /// 기록을 보관소로 옮긴다.
    fn archive(&self, id: &str) -> Result<(), ServerError>;
}

/// 이 도구가 새로 만든 키. 값은 저장소에 있고 여기에는 자리와 공개 키만 있다.
#[derive(Debug, Clone)]
pub struct CreatedKey {
    pub key: AccountKey,
    pub public_key: String,
    pub fingerprint: String,
}

/// 계정 키 파일들.
pub trait AccountKeys: Send + Sync {
    /// 이 서버 · 계정의 키 쌍을 새로 만든다.
    fn create(&self, server: &str, login: &str, comment: &str) -> Result<CreatedKey, ServerError>;
    /// 로컬의 키 파일을 시크릿 저장소로 복사한다. 원본은 그대로 둔다.
    fn import(&self, server: &str, login: &str, source: &str) -> Result<AccountKey, ServerError>;
    fn public_key(&self, server: &Server, key: &AccountKey) -> Result<String, ServerError>;
    /// ssh 에 넘길 개인 키의 절대 경로. `Agent` 면 없다. 파일이 없으면 오류다.
    fn private_path(
        &self,
        server: &Server,
        key: &AccountKey,
    ) -> Result<Option<String>, ServerError>;
    /// 만들다 실패한 키를 치운다.
    fn discard(&self, key: &AccountKey);
    /// 서버에서 제거한 계정의 키를 보관소로 옮긴다. 이 도구가 만든 키만 옮긴다.
    fn archive(&self, server: &str, key: &AccountKey) -> Result<(), ServerError>;
}

/// 서버에 들어가서 하는 일.
pub trait ServerGateway: Send + Sync {
    /// 서버가 계정을 받을 준비가 되었는지 본다. 아무것도 바꾸지 않는다.
    fn inspect(
        &self,
        admin: &Access,
        progress: &dyn crate::port::ProgressSink,
    ) -> Result<Readiness, ServerError>;

    /// 모자란 것을 채운다. 지금은 `acl` 하나다.
    fn prepare(
        &self,
        admin: &Access,
        progress: &dyn crate::port::ProgressSink,
    ) -> Result<Readiness, ServerError>;

    /// 계정 · 공용 자리 · 키 · 권한을 심는다. `New`는 서버에 같은 로그인이 있으면 거부하고,
    /// `Reinstall`은 기록된 키를 다시 심는다.
    /// 돌려주는 값은 **이번에 계정을 만들었는가**다.
    fn install(
        &self,
        admin: &Access,
        install: &Install,
        progress: &dyn crate::port::ProgressSink,
    ) -> Result<bool, ServerError>;

    /// 그 계정으로 실제로 들어가 본다. `check_sudo` 면 sudo 도 확인한다.
    fn verify(
        &self,
        access: &Access,
        check_sudo: bool,
        progress: &dyn crate::port::ProgressSink,
    ) -> Result<(), ServerError>;

    /// 이 도구가 심은 권한 · 키를 걷어낸다. `delete_account` 면 계정도 지운다.
    fn remove(
        &self,
        admin: &Access,
        login: &str,
        group: &str,
        delete_account: bool,
        progress: &dyn crate::port::ProgressSink,
    ) -> Result<(), ServerError>;
}

/// 리눅스 로그인 이름으로 쓸 수 있는 것만 받는다. 키 경로의 한 단계이기도 하다.
pub fn check_login(text: &str) -> Result<String, ServerError> {
    let login = text.trim();
    let valid = !login.is_empty()
        && login.len() <= 32
        && login.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        && login
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'));
    if valid {
        Ok(login.to_string())
    } else {
        Err(ServerError::Invalid(format!(
            "{login:?}은(는) 계정 이름으로 쓸 수 없습니다. 영문 소문자로 시작하고 소문자 · 숫자 · - · _만 씁니다."
        )))
    }
}

/// 주소. 터미널 명령에 그대로 들어가므로 셸이 달리 읽을 글자는 받지 않는다.
pub fn check_address(text: &str) -> Result<String, ServerError> {
    let address = text.trim();
    let valid = !address.is_empty()
        && !address.starts_with('-')
        && address
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '_'));
    if valid {
        Ok(address.to_string())
    } else {
        Err(ServerError::Invalid(
            "주소는 IP 또는 DNS 이름으로 입력하세요.".into(),
        ))
    }
}

/// 서버 이름. 화면에 보이는 이름이라 모양은 자유지만 비어 있거나 너무 길면 안 된다.
pub fn check_name(text: &str) -> Result<String, ServerError> {
    let name = text.trim();
    if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err(ServerError::Invalid(
            "서버 이름을 64자 안으로 입력하세요.".into(),
        ));
    }
    Ok(name.to_string())
}

/// 이름에서 id 를 만든다. 영문 소문자 · 숫자 · `-` 만 남기고, 남는 게 없으면 `server` 다.
/// `taken` 과 겹치면 `-2`, `-3` … 을 붙인다.
pub fn id_for(name: &str, taken: &[String]) -> String {
    let mut base: String = name
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while base.contains("--") {
        base = base.replace("--", "-");
    }
    let base = match base.trim_matches('-') {
        "" => "server".to_string(),
        trimmed => trimmed.to_string(),
    };
    if !taken.contains(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|candidate| !taken.contains(candidate))
        .expect("끝없는 수열에서 빈 자리는 반드시 나온다")
}

#[cfg(test)]
mod tests {
    use super::*;

    mod id_for {
        use super::*;

        #[test]
        fn keeps_ascii_words_and_joins_the_rest_with_dashes() {
            assert_eq!(id_for("tuk-api-server", &[]), "tuk-api-server");
            assert_eq!(id_for("Nemo Mac  mini", &[]), "nemo-mac-mini");
            assert_eq!(id_for("공구경", &[]), "server");
        }

        #[test]
        fn a_taken_id_gets_the_next_free_number() {
            let taken = vec!["nemo".to_string(), "nemo-2".to_string()];
            assert_eq!(id_for("nemo", &taken), "nemo-3");
        }
    }

    mod checks {
        use super::*;

        #[test]
        fn login_follows_linux_rules() {
            assert_eq!(check_login(" deploy ").unwrap(), "deploy");
            for bad in ["", "Deploy", "1ops", "a b", "a/b", "a.b"] {
                assert!(check_login(bad).is_err(), "{bad}");
            }
        }

        #[test]
        fn address_refuses_what_a_shell_would_read_differently() {
            assert_eq!(
                check_address("nemo.tail25dc19.ts.net").unwrap(),
                "nemo.tail25dc19.ts.net"
            );
            assert_eq!(check_address("fe80::1").unwrap(), "fe80::1");
            for bad in ["", "-oProxyCommand", "a b", "a;b", "$(id)", "a/b"] {
                assert!(check_address(bad).is_err(), "{bad}");
            }
        }
    }

    #[test]
    fn readiness_lists_what_is_missing_in_a_fixed_order() {
        let found = Readiness {
            sudo: true,
            acl: false,
            useradd: false,
            visudo: true,
            packager: None,
        };
        assert_eq!(found.missing(), vec!["useradd", "setfacl"]);
        assert!(!found.ok());
    }
}
