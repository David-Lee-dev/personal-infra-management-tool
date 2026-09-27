//! 서버 기록을 만들고 고치고 보관하는 일. 서버에는 닿지 않는다.
//!
//! 서버에 들어가는 일(계정 만들기 · 접속 확인)은 `provision` 이 한다.

use super::suggest::Suggestion;
use super::{
    AccountKey, AccountKeys, AccountOrigin, AccountState, AwsFacts, Role, Server, ServerAccount,
    ServerError, ServerKind, ServerStore, check_address, check_login, check_name, id_for,
};
use crate::port::Clock;

/// 원래 있던 계정을 기록할 때, 그 계정의 키.
#[derive(Debug, Clone)]
pub enum AccountChoice {
    /// 서버의 AWS 계정 · 리전에 있는 그 키 페어의 pem.
    Pem { keypair: String },
    /// 로컬 키 파일의 경로만 기록한다.
    File { path: String },
    /// 로컬 키 파일을 시크릿 저장소로 복사한다.
    Import { path: String },
    /// ssh 기본 키.
    Agent,
}

#[derive(Debug, Clone)]
pub struct NewAccount {
    pub login: String,
    pub role: Role,
    pub purpose: String,
    pub key: AccountChoice,
}

#[derive(Debug, Clone)]
pub struct NewServer {
    pub name: String,
    pub group: String,
    pub address: String,
    pub port: u16,
    pub kind: ServerKind,
    pub aws: Option<AwsFacts>,
    pub workspace: String,
    pub workspace_group: String,
    pub note: String,
    /// 처음 쓸 계정. 접속하려면 계정이 하나는 있어야 한다.
    pub account: NewAccount,
    /// 그 계정을 관리 접속으로 지정한다. 관리자 계정만 된다.
    pub admin_access: bool,
}

#[derive(Debug, Clone)]
pub struct ServerEdit {
    pub name: String,
    pub group: String,
    pub address: String,
    pub port: u16,
    pub kind: ServerKind,
    pub aws: Option<AwsFacts>,
    pub admin: Option<String>,
    pub workspace: String,
    pub workspace_group: String,
    pub note: String,
}

pub struct Servers<'a> {
    store: &'a dyn ServerStore,
    keys: &'a dyn AccountKeys,
    clock: &'a dyn Clock,
}

impl<'a> Servers<'a> {
    pub fn new(
        store: &'a dyn ServerStore,
        keys: &'a dyn AccountKeys,
        clock: &'a dyn Clock,
    ) -> Servers<'a> {
        Servers { store, keys, clock }
    }

    /// 전부. 그룹 있는 서버가 먼저, 그룹 · 이름 순. 읽지 못한 기록은 오류로 따로 준다.
    pub fn list(&self) -> (Vec<Server>, Vec<String>) {
        let mut found = Vec::new();
        let mut errors = Vec::new();
        for entry in self.store.list() {
            match entry {
                Ok(server) => found.push(server),
                Err(message) => errors.push(message),
            }
        }
        found.sort_by(|a, b| {
            (a.group.is_empty(), &a.group, &a.name).cmp(&(b.group.is_empty(), &b.group, &b.name))
        });
        (found, errors)
    }

    pub fn load(&self, id: &str) -> Result<Server, ServerError> {
        self.store.load(id)
    }

    /// 서버를 기록한다. 서버에는 아무것도 쓰지 않는다.
    ///
    /// 순서: 값 검사 → 겹침 검사 → 계정(키를 가져오면 여기서 복사) → 기록. 키 복사는 다른 검사가
    /// 다 끝난 뒤에 한다 — 기록하지 못할 서버의 키를 저장소에 남기지 않기 위해서다.
    pub fn register(&self, request: &NewServer) -> Result<Server, ServerError> {
        let mut server = Server {
            id: String::new(),
            name: check_name(&request.name)?,
            group: request.group.trim().to_string(),
            address: check_address(&request.address)?,
            port: check_port(request.port)?,
            kind: request.kind,
            admin: None,
            workspace: check_workspace(&request.workspace)?,
            workspace_group: check_login(&request.workspace_group)?,
            note: request.note.trim().to_string(),
            registered_at: self.clock.now(),
            aws: check_aws(request.kind, request.aws.as_ref())?,
            accounts: Vec::new(),
        };
        if request.admin_access && request.account.role != Role::Admin {
            return Err(ServerError::Invalid(
                "관리 접속은 관리자(sudo) 계정만 지정할 수 있습니다.".into(),
            ));
        }
        let servers = self.all();
        ensure_free(&servers, &server)?;
        server.id = id_for(
            &server.name,
            &servers.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
        );

        let account = self.recorded_account(&server, &request.account)?;
        if request.admin_access {
            server.admin = Some(account.login.clone());
        }
        server.accounts.push(account);
        self.store.insert(&server)?;
        Ok(server)
    }

    /// 제안받은 서버를 기록한다. 이름은 사람이 고친 것을 쓴다.
    pub fn adopt(&self, found: &Suggestion, name: &str) -> Result<Server, ServerError> {
        let mut server = Server {
            id: String::new(),
            name: check_name(name)?,
            group: String::new(),
            address: check_address(&found.address)?,
            port: check_port(found.port)?,
            kind: found.kind,
            admin: found.admin.clone(),
            workspace: found.workspace.clone(),
            workspace_group: found.workspace_group.clone(),
            note: String::new(),
            registered_at: self.clock.now(),
            aws: found.aws.clone(),
            accounts: found.accounts.clone(),
        };
        let servers = self.all();
        ensure_free(&servers, &server)?;
        server.id = id_for(
            &server.name,
            &servers.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
        );
        self.store.insert(&server)?;
        Ok(server)
    }

    /// 서버의 속성을 고친다. 계정은 그대로다.
    pub fn edit(&self, id: &str, edit: &ServerEdit) -> Result<Server, ServerError> {
        let mut server = self.store.load(id)?;
        server.name = check_name(&edit.name)?;
        server.group = edit.group.trim().to_string();
        server.address = check_address(&edit.address)?;
        server.port = check_port(edit.port)?;
        server.kind = edit.kind;
        server.aws = check_aws(edit.kind, edit.aws.as_ref())?;
        server.workspace = check_workspace(&edit.workspace)?;
        server.workspace_group = check_login(&edit.workspace_group)?;
        server.note = edit.note.trim().to_string();
        server.admin = match edit
            .admin
            .as_deref()
            .map(str::trim)
            .filter(|l| !l.is_empty())
        {
            None => None,
            Some(login) => Some(admin_candidate(&server, login)?),
        };
        let has_pem = server
            .accounts
            .iter()
            .any(|a| matches!(a.key, AccountKey::Pem { .. }));
        if has_pem && (!server.kind.is_aws() || server.aws.is_none()) {
            return Err(ServerError::Invalid(
                "pem 키로 들어가는 계정이 있어 AWS 서버 · AWS 계정 · 리전을 지워야 하는 변경은 할 수 없습니다.".into(),
            ));
        }
        let others: Vec<Server> = self
            .all()
            .into_iter()
            .filter(|s| s.id != server.id)
            .collect();
        ensure_free(&others, &server)?;
        self.store.replace(&server)?;
        Ok(server)
    }

    /// 서버 기록을 보관소로 옮긴다. 이 서버를 쓰는 프로젝트 환경(`used_by`)이 있으면 막는다.
    /// 서버의 계정과 키 파일은 건드리지 않는다.
    pub fn unregister(&self, id: &str, used_by: &[String]) -> Result<Server, ServerError> {
        let server = self.store.load(id)?;
        if !used_by.is_empty() {
            return Err(ServerError::Invalid(format!(
                "{}을(를) 쓰는 환경이 있어 등록을 해제할 수 없습니다: {}",
                server.name,
                used_by.join(", ")
            )));
        }
        self.store.archive(id)?;
        Ok(server)
    }

    /// 원래 있던 계정을 기록한다. 서버에는 쓰지 않는다.
    pub fn add_account(&self, id: &str, request: &NewAccount) -> Result<Server, ServerError> {
        let mut server = self.store.load(id)?;
        let login = check_login(&request.login)?;
        if server.account(&login).is_some() {
            return Err(ServerError::Taken(server.slug(&login)));
        }
        let account = self.recorded_account(&server, request)?;
        server.accounts.push(account);
        self.store.replace(&server)?;
        Ok(server)
    }

    /// 계정의 역할 · 용도를 고친다. 기록만 바꾼다 — 서버의 권한은 그대로다.
    pub fn edit_account(
        &self,
        id: &str,
        login: &str,
        role: Role,
        purpose: &str,
    ) -> Result<Server, ServerError> {
        let mut server = self.store.load(id)?;
        if role == Role::User && server.admin.as_deref() == Some(login) {
            return Err(ServerError::Invalid(
                "관리 접속 계정은 관리자여야 합니다. 관리 접속을 먼저 바꾸세요.".into(),
            ));
        }
        let slug = server.slug(login);
        let account = server
            .account_mut(login)
            .ok_or(ServerError::Missing(slug))?;
        account.role = role;
        account.purpose = purpose.trim().to_string();
        self.store.replace(&server)?;
        Ok(server)
    }

    /// 기록만 한 계정을 기록에서 뺀다. 이 도구가 키를 심은 계정은 서버에서 제거해야 한다.
    pub fn forget_account(
        &self,
        id: &str,
        login: &str,
        used_by: &[String],
    ) -> Result<Server, ServerError> {
        let mut server = self.store.load(id)?;
        let account = server
            .account(login)
            .ok_or_else(|| ServerError::Missing(server.slug(login)))?;
        if account.origin.managed() {
            return Err(ServerError::Invalid(format!(
                "{}은(는) 이 도구가 키를 심은 계정입니다. [서버에서 제거]를 쓰세요.",
                server.slug(login)
            )));
        }
        if !used_by.is_empty() {
            return Err(ServerError::Invalid(format!(
                "{}을(를) 쓰는 환경이 있습니다: {}",
                server.slug(login),
                used_by.join(", ")
            )));
        }
        server.accounts.retain(|a| a.login != login);
        if server.admin.as_deref() == Some(login) {
            server.admin = None;
        }
        self.store.replace(&server)?;
        Ok(server)
    }

    fn all(&self) -> Vec<Server> {
        self.store
            .list()
            .into_iter()
            .filter_map(Result::ok)
            .collect()
    }

    fn recorded_account(
        &self,
        server: &Server,
        request: &NewAccount,
    ) -> Result<ServerAccount, ServerError> {
        let login = check_login(&request.login)?;
        let key = match &request.key {
            AccountChoice::Agent => AccountKey::Agent,
            AccountChoice::Pem { keypair } => {
                let keypair = keypair.trim();
                if !server.kind.is_aws() || server.aws.is_none() {
                    return Err(ServerError::Invalid(
                        "pem 키는 AWS 계정 · 리전을 적은 AWS 서버에서만 쓸 수 있습니다.".into(),
                    ));
                }
                if keypair.is_empty() || keypair.contains(['/', '\\']) {
                    return Err(ServerError::Invalid("키 페어를 고르세요.".into()));
                }
                AccountKey::Pem {
                    keypair: keypair.to_string(),
                }
            }
            AccountChoice::File { path } => AccountKey::File {
                path: check_key_path(path)?,
            },
            AccountChoice::Import { path } => {
                self.keys
                    .import(&server.id, &login, &check_key_path(path)?)?
            }
        };
        Ok(ServerAccount {
            login,
            role: request.role,
            purpose: request.purpose.trim().to_string(),
            key,
            origin: AccountOrigin::Registered,
            state: AccountState::Unverified,
            verified_at: None,
            fingerprint: String::new(),
        })
    }
}

/// 같은 이름이나 같은 주소 · 포트의 서버가 둘이면 어느 쪽이 그 기계인지 헷갈린다.
fn ensure_free(servers: &[Server], server: &Server) -> Result<(), ServerError> {
    for other in servers {
        if other.name == server.name {
            return Err(ServerError::Taken(format!("서버 이름 {}", server.name)));
        }
        if other.address == server.address && other.port == server.port {
            return Err(ServerError::Taken(format!(
                "주소 {}:{} ({})",
                server.address, server.port, other.name
            )));
        }
    }
    Ok(())
}

fn admin_candidate(server: &Server, login: &str) -> Result<String, ServerError> {
    match server.account(login) {
        Some(account) if account.role == Role::Admin => Ok(login.to_string()),
        Some(_) => Err(ServerError::Invalid(format!(
            "{login}은(는) 관리자 계정이 아니라 관리 접속으로 지정할 수 없습니다."
        ))),
        None => Err(ServerError::Missing(server.slug(login))),
    }
}

fn check_port(port: u16) -> Result<u16, ServerError> {
    if port == 0 {
        return Err(ServerError::Invalid("포트를 확인하세요.".into()));
    }
    Ok(port)
}

fn check_workspace(text: &str) -> Result<String, ServerError> {
    let path = text.trim().trim_end_matches('/');
    let valid = path.starts_with('/')
        && path.len() > 1
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '-' | '_'));
    if valid {
        Ok(path.to_string())
    } else {
        Err(ServerError::Invalid(
            "공용 작업 디렉터리는 /로 시작하는 절대 경로여야 합니다.".into(),
        ))
    }
}

/// AWS 사실은 AWS 서버에만 둔다. 적었다면 계정과 리전은 있어야 pem 을 찾을 수 있다.
fn check_aws(kind: ServerKind, aws: Option<&AwsFacts>) -> Result<Option<AwsFacts>, ServerError> {
    if !kind.is_aws() {
        return Ok(None);
    }
    let Some(aws) = aws else {
        return Ok(None);
    };
    let facts = AwsFacts {
        account: aws.account.trim().to_string(),
        region: aws.region.trim().to_string(),
        instance: aws.instance.trim().to_string(),
    };
    if facts.account.is_empty() && facts.region.is_empty() && facts.instance.is_empty() {
        return Ok(None);
    }
    let plain = |v: &str| v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if facts.account.is_empty()
        || facts.region.is_empty()
        || ![&facts.account, &facts.region, &facts.instance]
            .iter()
            .all(|v| plain(v))
    {
        return Err(ServerError::Invalid(
            "AWS 계정 ID와 리전을 함께 입력하세요.".into(),
        ));
    }
    Ok(Some(facts))
}

fn check_key_path(text: &str) -> Result<String, ServerError> {
    let path = text.trim();
    if path.is_empty() {
        return Err(ServerError::Invalid("키 파일을 고르세요.".into()));
    }
    Ok(path.to_string())
}
