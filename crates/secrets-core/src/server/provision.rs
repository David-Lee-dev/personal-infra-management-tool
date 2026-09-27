//! 서버에 들어가서 하는 일 — 계정 만들기 · 다시 심기 · 제거 · 접속 확인.
//!
//! 서버가 끼어들어 한 번에 묶이지 않으므로, 각 단계 사이에서 죽어도 **무엇이 남았는지 화면이
//! 말할 수 있는 자리**에만 멈춘다.
//!
//! - 키를 먼저 만들고 서버에 나중에 심는다. 반대로 하면 개인 키 없는 공개 키가 서버에 남는다.
//! - 심은 뒤 **반드시 그 키로 들어가 본다.** 붙였다고 끝내면 못 들어가는 계정이 `확인됨`으로 남는다.
//! - 계정은 관리 접속(서버마다 지정한 sudo 계정)으로 들어가 만든다.

use super::{
    Access, AccountKeys, AccountOrigin, AccountState, Install, Readiness, Role, Server,
    ServerAccount, ServerError, ServerGateway, ServerStore, check_login,
};
use crate::port::{Clock, ProgressSink};

pub struct AccountProvisioning<'a> {
    store: &'a dyn ServerStore,
    keys: &'a dyn AccountKeys,
    gateway: &'a dyn ServerGateway,
    clock: &'a dyn Clock,
}

impl<'a> AccountProvisioning<'a> {
    pub fn new(
        store: &'a dyn ServerStore,
        keys: &'a dyn AccountKeys,
        gateway: &'a dyn ServerGateway,
        clock: &'a dyn Clock,
    ) -> AccountProvisioning<'a> {
        AccountProvisioning {
            store,
            keys,
            gateway,
            clock,
        }
    }

    /// 그 계정으로 들어가는 데 필요한 것. 키 파일이 없으면 오류다.
    pub fn access(&self, server: &Server, login: &str) -> Result<Access, ServerError> {
        let account = server
            .account(login)
            .ok_or_else(|| ServerError::Missing(server.slug(login)))?;
        Ok(Access {
            key: self.keys.private_path(server, &account.key)?,
            login: account.login.clone(),
            address: server.address.clone(),
            port: server.port,
        })
    }

    /// 서버가 계정을 받을 준비가 되었는지 관리 접속으로 본다. 아무것도 바꾸지 않는다.
    pub fn inspect(&self, id: &str, progress: &dyn ProgressSink) -> Result<Readiness, ServerError> {
        let server = self.store.load(id)?;
        self.gateway.inspect(&self.admin_access(&server)?, progress)
    }

    /// 모자란 것을 채운다. 패키지를 까는 유일한 자리라 사용자가 누를 때만 돈다.
    pub fn prepare(&self, id: &str, progress: &dyn ProgressSink) -> Result<Readiness, ServerError> {
        let server = self.store.load(id)?;
        self.gateway.prepare(&self.admin_access(&server)?, progress)
    }

    /// 서버에 계정을 새로 만들고 이 도구가 만든 키를 심은 뒤, 그 키로 들어가 확인한다.
    ///
    /// 순서: 준비 확인 → 키 → 기록(`Local`) → 심기 → 기록(`Installed`) → 확인(`Verified`).
    /// 심기가 실패하면 서버에 남은 것이 없으므로 키와 기록도 치운다.
    pub fn create(
        &self,
        id: &str,
        login: &str,
        role: Role,
        purpose: &str,
        progress: &dyn ProgressSink,
    ) -> Result<ServerAccount, ServerError> {
        let mut server = self.store.load(id)?;
        let login = check_login(login)?;
        if server.account(&login).is_some() {
            return Err(ServerError::Taken(server.slug(&login)));
        }
        let admin = self.admin_access(&server)?;
        let ready = self.gateway.inspect(&admin, progress)?;
        if !ready.ok() {
            return Err(ServerError::NotReady(ready.missing()));
        }

        let created = self.keys.create(
            &server.id,
            &login,
            &format!("secrets/{}/{login}", server.id),
        )?;
        let account = ServerAccount {
            login: login.clone(),
            role,
            purpose: purpose.trim().to_string(),
            key: created.key.clone(),
            origin: AccountOrigin::Installed,
            state: AccountState::Local,
            verified_at: None,
            fingerprint: created.fingerprint,
        };
        server.accounts.push(account);
        if let Err(e) = self.store.replace(&server) {
            self.keys.discard(&created.key);
            return Err(e);
        }

        let install = Install {
            login: login.clone(),
            role,
            workspace: server.workspace.clone(),
            group: server.workspace_group.clone(),
            public_key: created.public_key,
        };
        let made = match self.gateway.install(&admin, &install, progress) {
            Ok(made) => made,
            Err(e) => {
                server.accounts.retain(|a| a.login != login);
                let _ = self.store.replace(&server);
                self.keys.discard(&created.key);
                return Err(e);
            }
        };
        self.update(&mut server, &login, |a| {
            a.origin = if made {
                AccountOrigin::Created
            } else {
                AccountOrigin::Installed
            };
            a.state = AccountState::Installed;
        })?;
        self.confirm(&mut server, &login, progress)
    }

    /// 같은 키로 다시 심는다. 멈춘 자리에서 이어 갈 때도, 서버 설정이 바뀌었을 때도 쓴다.
    ///
    /// 스크립트는 몇 번 돌려도 같은 결과라 `확인됨`인 계정에도 돌릴 수 있다.
    pub fn reinstall(
        &self,
        id: &str,
        login: &str,
        progress: &dyn ProgressSink,
    ) -> Result<ServerAccount, ServerError> {
        let mut server = self.store.load(id)?;
        let account = managed(&server, login)?.clone();
        let admin = self.admin_access(&server)?;
        let ready = self.gateway.inspect(&admin, progress)?;
        if !ready.ok() {
            return Err(ServerError::NotReady(ready.missing()));
        }
        let install = Install {
            login: account.login.clone(),
            role: account.role,
            workspace: server.workspace.clone(),
            group: server.workspace_group.clone(),
            public_key: self.keys.public_key(&server, &account.key)?,
        };
        let made = self.gateway.install(&admin, &install, progress)?;
        // "계정을 만들었나"는 처음 한 번 정해진다. 다시 심을 때는 계정이 이미 있어 거짓이 오는데,
        // 그대로 적으면 이 도구가 만든 계정을 나중에 지우지 못한다.
        self.update(&mut server, login, |a| {
            if made {
                a.origin = AccountOrigin::Created;
            }
            a.state = AccountState::Installed;
        })?;
        self.confirm(&mut server, login, progress)
    }

    /// 이 도구가 심은 키 · 권한을 서버에서 걷고 기록을 뺀다. 이 도구가 만든 계정이면 계정도 지운다.
    /// 이 계정을 쓰는 프로젝트 환경(`used_by`)이 있으면 막는다.
    ///
    /// 서버에서 먼저 지운다 — 기록을 먼저 치우면 무엇을 어디서 지워야 하는지 잃는다.
    pub fn remove(
        &self,
        id: &str,
        login: &str,
        used_by: &[String],
        progress: &dyn ProgressSink,
    ) -> Result<(), ServerError> {
        let mut server = self.store.load(id)?;
        let account = managed(&server, login)?.clone();
        if !used_by.is_empty() {
            return Err(ServerError::Invalid(format!(
                "{}을(를) 쓰는 환경이 있습니다: {}",
                server.slug(login),
                used_by.join(", ")
            )));
        }
        if server.admin.as_deref() == Some(login) {
            return Err(ServerError::Invalid(
                "관리 접속 계정은 서버에서 제거할 수 없습니다. 관리 접속을 먼저 바꾸세요.".into(),
            ));
        }
        let admin = self.admin_access(&server)?;
        self.gateway.remove(
            &admin,
            login,
            &server.workspace_group,
            account.origin == AccountOrigin::Created,
            progress,
        )?;
        server.accounts.retain(|a| a.login != login);
        self.store.replace(&server)?;
        self.keys.archive(&server.id, &account.key)
    }

    /// 그 계정으로 들어가 본다. 서버에는 아무것도 쓰지 않는다.
    ///
    /// 들어가지 못하면 오류를 돌려준다. 기록만 한 계정은 `확인 전`으로 되돌려 둔다.
    pub fn check(
        &self,
        id: &str,
        login: &str,
        progress: &dyn ProgressSink,
    ) -> Result<ServerAccount, ServerError> {
        let mut server = self.store.load(id)?;
        let access = self.access(&server, login)?;
        match self.gateway.verify(&access, false, progress) {
            Ok(()) => {
                let now = self.clock.now();
                self.update(&mut server, login, |a| {
                    a.state = AccountState::Verified;
                    a.verified_at = Some(now);
                })
            }
            Err(e) => {
                let registered = server
                    .account(login)
                    .is_some_and(|a| a.origin == AccountOrigin::Registered);
                if registered {
                    self.update(&mut server, login, |a| a.state = AccountState::Unverified)?;
                }
                Err(e)
            }
        }
    }

    fn confirm(
        &self,
        server: &mut Server,
        login: &str,
        progress: &dyn ProgressSink,
    ) -> Result<ServerAccount, ServerError> {
        let access = self.access(server, login)?;
        let check_sudo = server.account(login).is_some_and(|a| a.role == Role::Admin);
        self.gateway.verify(&access, check_sudo, progress)?;
        let now = self.clock.now();
        self.update(server, login, |a| {
            a.state = AccountState::Verified;
            a.verified_at = Some(now);
        })
    }

    fn admin_access(&self, server: &Server) -> Result<Access, ServerError> {
        let login = server.admin.as_deref().ok_or_else(|| {
            ServerError::Invalid(format!(
                "{}에 관리 접속이 지정되지 않았습니다. 관리자 계정을 등록하고 관리 접속으로 지정하세요.",
                server.name
            ))
        })?;
        self.access(server, login)
    }

    /// 계정 하나를 고쳐 기록하고 고친 계정을 돌려준다.
    fn update(
        &self,
        server: &mut Server,
        login: &str,
        change: impl FnOnce(&mut ServerAccount),
    ) -> Result<ServerAccount, ServerError> {
        let slug = server.slug(login);
        let account = server
            .account_mut(login)
            .ok_or(ServerError::Missing(slug))?;
        change(account);
        let changed = account.clone();
        self.store.replace(server)?;
        Ok(changed)
    }
}

/// 이 도구가 키를 심은 계정만 다시 심거나 서버에서 제거할 수 있다.
fn managed<'s>(server: &'s Server, login: &str) -> Result<&'s ServerAccount, ServerError> {
    let account = server
        .account(login)
        .ok_or_else(|| ServerError::Missing(server.slug(login)))?;
    if !account.origin.managed() {
        return Err(ServerError::Invalid(format!(
            "{}은(는) 기록만 한 계정입니다. 이 도구가 심은 키가 없습니다.",
            server.slug(login)
        )));
    }
    Ok(account)
}
