//! 서버 — 등록 · 편집 · 해제, 계정 기록 · 만들기 · 제거, 그리고 Ghostty 로 접속.
//!
//! 서버에 들어가는 일은 작업 로그에 그대로 흐른다. 목록이 바뀌었다는 알림은 **바꾼 명령만**
//! 보낸다. 읽기만 하는 명령이 보내면 화면이 다시 그려지면서 방금 보여 준 결과를 지운다.

use secrets_core::project::ProjectStore;
use secrets_core::server::suggest::{Suggestion, suggest};
use secrets_core::server::{
    AccountChoice, AccountKey, AccountOrigin, AccountState, AwsFacts, NewAccount, NewServer,
    Readiness, Role, Server, ServerAccount, ServerEdit, ServerError, ServerKind, ServerStore,
};
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::DialogExt;

use crate::command::keys::tilde;
use crate::dto::*;
use crate::progress::{Ended, JobPanel, Started, next_job_id};
use crate::wiring::Wiring;

const UPDATED: &str = "servers:updated";

fn error(e: ServerError) -> String {
    e.to_string()
}

/// 서버 기록이 바뀌었다. 서버 목록과, 서버를 보여 주는 프로젝트 · 자격 증명 화면이 다시 읽는다.
fn changed(app: &AppHandle) {
    let _ = app.emit(UPDATED, ());
    let _ = app.emit("projects:updated", ());
    let _ = app.emit("keys:updated", ());
}

/// 오래 걸리는 일을 하나의 job 으로 감싸 작업 로그에 흘린다.
fn job<T>(
    app: &AppHandle,
    label: String,
    work: impl FnOnce(&JobPanel) -> Result<T, String>,
) -> Result<T, String> {
    let id = next_job_id();
    let _ = app.emit(
        "cli:start",
        Started {
            job: id.clone(),
            command: label.clone(),
        },
    );
    let panel = JobPanel {
        app: app.clone(),
        job: id.clone(),
    };
    let result = work(&panel);
    let _ = app.emit(
        "cli:end",
        Ended {
            job: id,
            ok: result.is_ok(),
            message: match &result {
                Ok(_) => label,
                Err(e) => e.clone(),
            },
        },
    );
    result
}

/* ── 표현 ─────────────────────────────────────────────── */

fn role(role: Role) -> &'static str {
    role.id()
}

fn role_of(text: &str) -> Role {
    match text {
        "admin" => Role::Admin,
        _ => Role::User,
    }
}

fn kind_of(text: &str) -> Result<ServerKind, String> {
    ServerKind::parse(text).ok_or_else(|| format!("서버 종류를 알 수 없습니다: {text}"))
}

fn origin(origin: AccountOrigin) -> &'static str {
    match origin {
        AccountOrigin::Created => "created",
        AccountOrigin::Installed => "installed",
        AccountOrigin::Registered => "registered",
    }
}

fn state(state: AccountState) -> &'static str {
    state.id()
}

fn key_label(account: &ServerAccount) -> String {
    match &account.key {
        AccountKey::Vault { .. } if account.origin.managed() => "이 도구가 만든 키".into(),
        AccountKey::Vault { .. } => "시크릿 저장소로 가져온 키".into(),
        AccountKey::Pem { keypair } => format!("pem {keypair}"),
        AccountKey::File { path } => path.clone(),
        AccountKey::Agent => "ssh 기본 키".into(),
    }
}

fn aws_row(aws: &AwsFacts) -> AwsFactsRow {
    AwsFactsRow {
        account: aws.account.clone(),
        region: aws.region.clone(),
        instance: aws.instance.clone(),
    }
}

fn aws_facts(row: &Option<AwsFactsRow>) -> Option<AwsFacts> {
    row.as_ref().map(|a| AwsFacts {
        account: a.account.clone(),
        region: a.region.clone(),
        instance: a.instance.clone(),
    })
}

/// 프로젝트 환경이 서버를 어떻게 쓰는지 — 서버 id 로 찾는다.
struct Uses(Vec<(String, secrets_core::project::Environment)>);

impl Uses {
    fn read() -> Uses {
        Uses(
            Wiring::get()
                .project_store()
                .list()
                .into_iter()
                .filter_map(Result::ok)
                .flat_map(|r| {
                    let project = r.name.clone();
                    r.environments
                        .into_iter()
                        .map(move |e| (project.clone(), e))
                })
                .collect(),
        )
    }

    fn of(&self, server: &str) -> Vec<ServerUseRow> {
        self.0
            .iter()
            .filter(|(_, e)| e.server == server)
            .map(|(project, e)| ServerUseRow {
                project: project.clone(),
                environment: e.name.clone(),
                login: e.login.clone(),
                path: e.path.clone(),
                branch: e.branch.clone(),
            })
            .collect()
    }
}

fn server_row(server: &Server, uses: &Uses) -> ServerRow {
    let used = uses.of(&server.id);
    ServerRow {
        id: server.id.clone(),
        name: server.name.clone(),
        group: server.group.clone(),
        address: server.address.clone(),
        port: server.port,
        kind: server.kind.id(),
        admin: server.admin.clone(),
        workspace: server.workspace.clone(),
        workspace_group: server.workspace_group.clone(),
        note: server.note.clone(),
        registered_at: server
            .registered_at
            .get(..16)
            .unwrap_or(&server.registered_at)
            .replace('T', " "),
        aws: server.aws.as_ref().map(aws_row),
        accounts: server
            .accounts
            .iter()
            .map(|a| account_row(server, a, &used))
            .collect(),
        uses: used,
    }
}

fn account_row(
    server: &Server,
    account: &ServerAccount,
    used: &[ServerUseRow],
) -> ServerAccountRow {
    ServerAccountRow {
        login: account.login.clone(),
        role: role(account.role),
        purpose: account.purpose.clone(),
        key_kind: account.key.id(),
        key_label: key_label(account),
        pem: match &account.key {
            AccountKey::Pem { keypair } => Some(keypair.clone()),
            _ => None,
        },
        origin: origin(account.origin),
        state: state(account.state),
        verified_at: account
            .verified_at
            .as_ref()
            .map(|t| t.get(..16).unwrap_or(t).replace('T', " ")),
        admin_access: server.admin.as_deref() == Some(account.login.as_str()),
        used_by: used
            .iter()
            .filter(|u| u.login == account.login)
            .map(|u| format!("{}/{}", u.project, u.environment))
            .collect(),
    }
}

fn readiness(found: &Readiness) -> HostReadiness {
    HostReadiness {
        ok: found.ok(),
        missing: found.missing().iter().map(|m| m.to_string()).collect(),
        sudo: found.sudo,
        acl: found.acl,
        useradd: found.useradd,
        visudo: found.visudo,
        packager: found.packager.clone(),
    }
}

/// 이 서버(계정을 주면 그 계정)를 쓰는 환경. 해제 · 제거를 막는 근거다.
fn used_by(server: &str, login: Option<&str>) -> Vec<String> {
    Wiring::get().server_link().used_by(server, login)
}

/* ── 제안 ─────────────────────────────────────────────── */

fn suggestions() -> Vec<Suggestion> {
    let (registered, _) = Wiring::get().servers().list();
    suggest(
        &secrets_local::server::legacy::legacy_accounts(),
        &secrets_local::server::legacy::config_hosts(),
        &registered,
        &secrets_local::vault::root().display().to_string(),
    )
}

fn suggestion_key(found: &Suggestion) -> String {
    format!("{}:{}", found.address, found.port)
}

fn instance_of(found: &Suggestion) -> Option<&str> {
    found
        .aws
        .as_ref()
        .map(|a| a.instance.as_str())
        .filter(|i| !i.is_empty())
}

fn suggestion_row(found: &Suggestion) -> SuggestionRow {
    SuggestionRow {
        key: suggestion_key(found),
        name: found.name.clone(),
        address: found.address.clone(),
        port: found.port,
        kind: found.kind.id(),
        admin: found.admin.clone(),
        accounts: found
            .accounts
            .iter()
            .map(|a| SuggestedAccountRow {
                login: a.login.clone(),
                role: role(a.role),
                key_kind: a.key.id(),
                key_label: key_label(a),
            })
            .collect(),
        sources: found.sources.clone(),
        skipped: found.skipped.clone(),
        links: Wiring::get()
            .server_link()
            .legacy_users(instance_of(found), &found.address),
    }
}

/* ── 읽기 ─────────────────────────────────────────────── */

/// 등록된 서버 전부와, 등록하지 않은 서버 제안의 수.
#[tauri::command]
pub async fn list_servers() -> ServerListRow {
    let wiring = Wiring::get();
    let (servers, errors) = wiring.servers().list();
    let uses = Uses::read();

    let mut groups: Vec<String> = wiring
        .project_store()
        .list()
        .into_iter()
        .filter_map(Result::ok)
        .map(|r| r.group)
        .chain(servers.iter().map(|s| s.group.clone()))
        .filter(|g| !g.is_empty())
        .collect();
    groups.sort();
    groups.dedup();

    ServerListRow {
        servers: servers.iter().map(|s| server_row(s, &uses)).collect(),
        errors,
        suggestions: suggestions().len(),
        groups,
    }
}

#[tauri::command]
pub async fn server_detail(id: String) -> Result<ServerRow, String> {
    let server = Wiring::get().servers().load(&id).map_err(error)?;
    Ok(server_row(&server, &Uses::read()))
}

/// 등록하지 않은 서버. 읽기만 한다.
#[tauri::command]
pub async fn server_suggestions() -> Vec<SuggestionRow> {
    suggestions().iter().map(suggestion_row).collect()
}

/// 다른 서버에 이미 있는 계정 이름과 sudo 여부. 계정을 만들 때 같은 이름을 빨리 넣게 보여 준다.
#[tauri::command]
pub async fn known_server_accounts() -> Vec<KnownAccountRow> {
    let (servers, _) = Wiring::get().servers().list();
    let mut found: Vec<KnownAccountRow> = Vec::new();
    for account in servers.iter().flat_map(|s| s.accounts.iter()) {
        let admin = account.role == Role::Admin;
        match found
            .iter_mut()
            .find(|k| k.login == account.login && k.admin == admin)
        {
            Some(known) => known.servers += 1,
            None => found.push(KnownAccountRow {
                login: account.login.clone(),
                admin,
                servers: 1,
            }),
        }
    }
    found.sort_by(|a, b| b.servers.cmp(&a.servers).then(a.login.cmp(&b.login)));
    found
}

/* ── 등록 · 편집 · 해제 ─────────────────────────────────── */

fn account_request(form: &ServerAccountForm) -> Result<NewAccount, String> {
    let value = form.key.value.trim().to_string();
    let key = match form.key.kind.as_str() {
        "pem" => AccountChoice::Pem { keypair: value },
        "file" => AccountChoice::File { path: value },
        "import" => AccountChoice::Import { path: value },
        "agent" => AccountChoice::Agent,
        other => return Err(format!("키 종류를 알 수 없습니다: {other}")),
    };
    Ok(NewAccount {
        login: form.login.clone(),
        role: role_of(&form.role),
        purpose: form.purpose.clone(),
        key,
    })
}

/// 등록한 서버를 가리키는 옛 환경(같은 인스턴스 · 주소)을 그 서버로 잇는다.
fn adopt_environments(server: &Server) -> Result<Vec<String>, String> {
    let instance = server
        .aws
        .as_ref()
        .map(|a| a.instance.as_str())
        .filter(|i| !i.is_empty());
    Wiring::get()
        .server_link()
        .adopt_legacy(&server.id, instance, &server.address)
        .map_err(|e| e.to_string())
}

/// 서버를 기록한다. 서버에는 쓰지 않는다.
#[tauri::command]
pub async fn register_server(
    app: AppHandle,
    form: ServerRegisterForm,
) -> Result<ServerRow, String> {
    let fields = &form.server;
    let request = NewServer {
        name: fields.name.clone(),
        group: fields.group.clone(),
        address: fields.address.clone(),
        port: fields.port,
        kind: kind_of(&fields.kind)?,
        aws: aws_facts(&fields.aws),
        workspace: fields.workspace.clone(),
        workspace_group: fields.workspace_group.clone(),
        note: fields.note.clone(),
        account: account_request(&form.account)?,
        admin_access: form.admin_access,
    };
    let server = Wiring::get()
        .servers()
        .register(&request)
        .map_err(|e| match e {
            ServerError::Taken(_) => format!(
                "{e} 같은 이름이나 주소의 서버가 이미 목록에 있습니다. 같은 기계라면 새로 등록하지 말고 \
                 목록에서 그 서버를 열어 [＋ 계정]으로 계정을 더하세요. 그 기록이 틀렸다면 등록 해제한 뒤 다시 등록하세요."
            ),
            e => error(e),
        })?;
    // 서버는 이미 기록됐다. 옛 환경을 잇지 못해도 목록은 다시 읽게 하고, 그 사실을 알린다.
    let linked = adopt_environments(&server);
    changed(&app);
    linked.map_err(|e| {
        format!(
            "{}을(를) 등록했지만 옛 환경을 잇지 못했습니다: {e}",
            server.name
        )
    })?;
    Ok(server_row(&server, &Uses::read()))
}

/// 고른 제안을 등록하고, 그 서버를 가리키던 옛 환경을 잇는다. 하나가 실패해도 나머지는 등록한다.
#[tauri::command]
pub async fn adopt_servers(app: AppHandle, picks: Vec<AdoptPick>) -> AdoptedRow {
    let found = suggestions();
    let mut done = AdoptedRow {
        registered: Vec::new(),
        linked: Vec::new(),
        errors: Vec::new(),
    };
    for pick in &picks {
        let Some(suggestion) = found.iter().find(|s| suggestion_key(s) == pick.key) else {
            done.errors
                .push(format!("{}: 제안을 다시 찾지 못했습니다.", pick.key));
            continue;
        };
        let server = match Wiring::get().servers().adopt(suggestion, &pick.name) {
            Ok(server) => server,
            Err(e) => {
                done.errors.push(format!("{}: {e}", pick.name));
                continue;
            }
        };
        match adopt_environments(&server) {
            Ok(linked) => done.linked.extend(linked),
            Err(e) => done.errors.push(format!(
                "{}: 등록했지만 옛 환경을 잇지 못했습니다: {e}",
                server.name
            )),
        }
        done.registered.push(server.name);
    }
    if !done.registered.is_empty() {
        changed(&app);
    }
    done
}

#[tauri::command]
pub async fn update_server(
    app: AppHandle,
    id: String,
    form: ServerFields,
) -> Result<ServerRow, String> {
    let edit = ServerEdit {
        name: form.name.clone(),
        group: form.group.clone(),
        address: form.address.clone(),
        port: form.port,
        kind: kind_of(&form.kind)?,
        aws: aws_facts(&form.aws),
        admin: form.admin.clone(),
        workspace: form.workspace.clone(),
        workspace_group: form.workspace_group.clone(),
        note: form.note.clone(),
    };
    let server = Wiring::get().servers().edit(&id, &edit).map_err(error)?;
    changed(&app);
    Ok(server_row(&server, &Uses::read()))
}

/// 서버 기록을 보관소로 옮긴다. 이 서버를 쓰는 환경이 있으면 막는다. 서버와 키 파일은 그대로다.
#[tauri::command]
pub async fn unregister_server(app: AppHandle, id: String) -> Result<(), String> {
    let uses = used_by(&id, None);
    Wiring::get()
        .servers()
        .unregister(&id, &uses)
        .map_err(|e| {
            if uses.is_empty() {
                return error(e);
            }
            format!(
                "{e} 프로젝트에서 그 환경의 편집을 열어 다른 서버로 바꾸거나 [환경 빼기]를 한 뒤 해제하세요."
            )
        })?;
    changed(&app);
    Ok(())
}

/* ── 계정 기록 ────────────────────────────────────────── */

/// 원래 있던 계정을 기록한다. 서버에는 쓰지 않는다.
#[tauri::command]
pub async fn add_server_account(
    app: AppHandle,
    id: String,
    form: ServerAccountForm,
) -> Result<ServerRow, String> {
    let server = Wiring::get()
        .servers()
        .add_account(&id, &account_request(&form)?)
        .map_err(error)?;
    changed(&app);
    Ok(server_row(&server, &Uses::read()))
}

#[tauri::command]
pub async fn edit_server_account(
    app: AppHandle,
    id: String,
    login: String,
    role: String,
    purpose: String,
) -> Result<ServerRow, String> {
    let server = Wiring::get()
        .servers()
        .edit_account(&id, &login, role_of(&role), &purpose)
        .map_err(error)?;
    changed(&app);
    Ok(server_row(&server, &Uses::read()))
}

/// 기록만 한 계정을 기록에서 뺀다. 서버의 계정은 그대로다.
#[tauri::command]
pub async fn forget_server_account(
    app: AppHandle,
    id: String,
    login: String,
) -> Result<ServerRow, String> {
    let server = Wiring::get()
        .servers()
        .forget_account(&id, &login, &used_by(&id, Some(&login)))
        .map_err(error)?;
    changed(&app);
    Ok(server_row(&server, &Uses::read()))
}

/// 키 파일을 Finder 에서 고른다. `~/.ssh` 에서 시작한다. 취소하면 `None`.
#[tauri::command]
pub async fn pick_key_file(app: AppHandle) -> Result<Option<String>, String> {
    let start = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".ssh");
    let Some(picked) = app
        .dialog()
        .file()
        .set_title("개인 키 파일 고르기")
        .set_directory(&start)
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let picked = picked.into_path().map_err(|e| e.to_string())?;
    Ok(Some(tilde(&picked)))
}

/* ── 접속 ─────────────────────────────────────────────── */

fn access(id: &str, login: &str) -> Result<secrets_core::server::Access, String> {
    let wiring = Wiring::get();
    let server = wiring.server_store().load(id).map_err(error)?;
    wiring.provisioning().access(&server, login).map_err(error)
}

/// Ghostty 새 창에서 그 계정으로 접속한다. 창을 연 뒤 곧바로 돌아온다.
#[tauri::command]
pub async fn connect_server_account(id: String, login: String) -> Result<String, String> {
    let access = access(&id, &login)?;
    secrets_local::hosts::terminal::open_ssh(&access).map_err(error)?;
    Ok(format!("{}@{}", access.login, access.address))
}

/// 그 계정으로 접속하는 ssh 명령 한 줄. 화면이 클립보드에 넣는다.
#[tauri::command]
pub async fn server_ssh_command(id: String, login: String) -> Result<String, String> {
    secrets_local::hosts::terminal::ssh_command(&access(&id, &login)?).map_err(error)
}

/// 그 계정으로 들어가 본다. 서버에는 아무것도 쓰지 않는다.
#[tauri::command]
pub async fn check_server_account(
    app: AppHandle,
    id: String,
    login: String,
) -> Result<ServerRow, String> {
    let done = job(&app, format!("{id}/{login} 접속 확인"), |panel| {
        Wiring::get()
            .provisioning()
            .check(&id, &login, panel)
            .map_err(error)
    });
    // 들어가지 못해도 상태가 바뀌었을 수 있다(확인 전으로). 결과와 상관없이 다시 읽게 한다.
    changed(&app);
    done?;
    server_detail(id).await
}

/* ── 서버에 계정 만들기 ────────────────────────────────── */

/// 이 서버가 계정을 받을 준비가 되었는지 관리 접속으로 본다. 아무것도 바꾸지 않는다.
#[tauri::command]
pub async fn inspect_server(app: AppHandle, id: String) -> Result<HostReadiness, String> {
    job(&app, format!("{id} 점검"), |panel| {
        Wiring::get()
            .provisioning()
            .inspect(&id, panel)
            .map(|found| readiness(&found))
            .map_err(error)
    })
}

/// 모자란 것을 채운다. 지금은 `acl` 하나다. 패키지를 까는 유일한 자리라 사용자가 누를 때만 돈다.
#[tauri::command]
pub async fn prepare_server(app: AppHandle, id: String) -> Result<HostReadiness, String> {
    job(&app, format!("{id} 준비"), |panel| {
        Wiring::get()
            .provisioning()
            .prepare(&id, panel)
            .map(|found| readiness(&found))
            .map_err(error)
    })
}

/// 계정을 여럿 만든다. 앞에서부터 하나씩, 하나가 실패해도 나머지는 만든다.
/// 목록 갱신은 끝에 한 번만 알린다 — 도중에 알리면 화면이 폼을 다시 그린다.
#[tauri::command]
pub async fn create_server_accounts(
    app: AppHandle,
    id: String,
    accounts: Vec<NewSeat>,
) -> Result<Vec<SeatOutcome>, String> {
    if accounts.is_empty() {
        return Err("생성할 계정이 없습니다.".into());
    }
    let mut outcomes = Vec::new();
    for wanted in &accounts {
        let done = job(
            &app,
            format!("{id}/{} 계정 만들기", wanted.account.trim()),
            |panel| {
                Wiring::get()
                    .provisioning()
                    .create(
                        &id,
                        &wanted.account,
                        role_of(&wanted.role),
                        &wanted.purpose,
                        panel,
                    )
                    .map(|_| ())
                    .map_err(error)
            },
        );
        outcomes.push(SeatOutcome {
            account: wanted.account.trim().to_string(),
            error: done.err(),
        });
    }
    if outcomes.iter().any(|o| o.error.is_none()) {
        changed(&app);
    }
    Ok(outcomes)
}

/// 같은 키로 다시 심는다. 멈춘 자리에서도, 서버 설정이 바뀌었을 때도 쓴다.
#[tauri::command]
pub async fn reinstall_server_account(
    app: AppHandle,
    id: String,
    login: String,
) -> Result<(), String> {
    let done = job(
        &app,
        format!("{id}/{login} SSH 키 다시 등록"),
        |panel| {
            Wiring::get()
                .provisioning()
                .reinstall(&id, &login, panel)
                .map(|_| ())
                .map_err(error)
        },
    );
    changed(&app);
    done
}

/// 이 도구가 심은 키 · 권한을 서버에서 걷고 기록을 뺀다. 이 도구가 만든 계정이면 계정도 지운다.
#[tauri::command]
pub async fn remove_server_account(
    app: AppHandle,
    id: String,
    login: String,
) -> Result<(), String> {
    let used = used_by(&id, Some(&login));
    let done = job(&app, format!("{id}/{login} 서버에서 제거"), |panel| {
        Wiring::get()
            .provisioning()
            .remove(&id, &login, &used, panel)
            .map_err(error)
    });
    if done.is_ok() {
        changed(&app);
    }
    done
}
