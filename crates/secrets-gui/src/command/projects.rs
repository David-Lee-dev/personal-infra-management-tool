//! 프로젝트 — 목록 · 새로 만들기 · 등록 · Git 연결 · 서버 연결 · 코드 받기.
//!
//! 사용자의 작업 공간에 쓰는 일은 새 디렉토리 만들기 · `git init` · Git 연결의 로컬 설정뿐이다.
//! 서버에 쓰는 일은 코드 받기(비어 있는 배포 경로에만)와 .env 반영(배포 경로 뿌리의 환경 변수 파일 하나)이다. 등록은 기록만
//! 남긴다. 목록과 상세는 디렉토리를 스캔하므로(git 질의) 비동기로 돌려 화면을 막지 않는다.

use secrets_core::key::RepoRef;
use secrets_core::project::{
    Blocker, Checkout, CodeNote, EnvComparison, EnvFileRole, EnvState, Environment,
    EnvironmentEdit, GitRequest, GitStart, GitState, LocalScan, NewProject, Origin, Overview,
    PathState, ProjectEdit, ProjectError, ProjectRecord, Registration, RemoteChoice, RepoTracking,
    Revision, ServerRequest, StageState, Stages, Visibility,
};
use secrets_local::project::{absolute, inside, workspace_root};
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::DialogExt;

use crate::command::keys::tilde;
use crate::dto::*;
use crate::progress::{Ended, JobPanel, Started, next_job_id};
use crate::wiring::Wiring;

const UPDATED: &str = "projects:updated";

fn stage(state: StageState) -> &'static str {
    match state {
        StageState::Done => "done",
        StageState::Warn => "warn",
        StageState::Pending => "pending",
    }
}

fn stages(of: &Stages) -> StagesRow {
    StagesRow {
        local: stage(of.local),
        git: stage(of.git),
        server: stage(of.server),
    }
}

fn git_row(state: &GitState) -> GitRow {
    match state {
        GitState::Absent => GitRow {
            kind: "absent",
            branch: None,
            commits: 0,
            changes: 0,
            origin: None,
            repo: None,
            ssh_key: None,
        },
        GitState::Local {
            branch,
            commits,
            changes,
        } => GitRow {
            kind: "local",
            branch: branch.clone(),
            commits: *commits,
            changes: *changes,
            origin: None,
            repo: None,
            ssh_key: None,
        },
        GitState::Remote {
            branch,
            commits,
            changes,
            origin,
        } => GitRow {
            kind: "remote",
            branch: branch.clone(),
            commits: *commits,
            changes: *changes,
            origin: Some(origin.clone()),
            repo: RepoRef::parse(origin).map(|r| r.slug()),
            ssh_key: None,
        },
    }
}

fn scan_row(scan: &Result<LocalScan, ProjectError>) -> ScanRow {
    let scan = match scan {
        Ok(scan) => scan,
        Err(e) => {
            return ScanRow {
                git: None,
                runtimes: Vec::new(),
                installable: false,
                env_files: Vec::new(),
                error: Some(e.to_string()),
            };
        }
    };
    let verdict = scan.runtime();
    ScanRow {
        git: Some(GitRow {
            ssh_key: scan
                .ssh_key
                .as_deref()
                .map(|p| tilde(std::path::Path::new(p))),
            ..git_row(&scan.git)
        }),
        runtimes: verdict
            .detected
            .iter()
            .map(|d| RuntimeRow {
                label: d.runtime.label(),
                version: d.version.clone(),
                sources: d.sources.clone(),
                conflict: d.conflict,
                package_manager: d.runtime.is_package_manager(),
            })
            .collect(),
        installable: verdict.installable(),
        env_files: scan
            .env_view()
            .iter()
            .map(|f| {
                let (role, env) = match &f.role {
                    EnvFileRole::Example => ("example", None),
                    EnvFileRole::Local => ("local", None),
                    EnvFileRole::Environment(name) => ("environment", Some(name.clone())),
                    EnvFileRole::Other => ("other", None),
                };
                EnvFileRow {
                    name: f.name.clone(),
                    role,
                    env,
                    variables: f.variables,
                    tracked: f.tracked,
                    ignored: f.ignored,
                    exposed: f.exposed(),
                }
            })
            .collect(),
        error: None,
    }
}

fn row(overview: &Overview) -> ProjectRow {
    let record = &overview.record;
    ProjectRow {
        name: record.name.clone(),
        group: record.group.clone(),
        path: tilde(std::path::Path::new(&record.path)),
        absolute: record.path.clone(),
        origin: match record.origin {
            Origin::Created => "created",
            Origin::Registered => "registered",
        },
        created_at: record
            .created_at
            .get(..16)
            .unwrap_or(&record.created_at)
            .replace('T', " "),
        stages: stages(&overview.stages),
        scan: scan_row(&overview.scan),
        environments: record
            .environments
            .iter()
            .map(|env| environment_row(&record.name, env))
            .collect(),
    }
}

/// 환경이 가리키는 서버의 이름 · 종류 · 주소. 서버 기록이 생기기 전의 환경이면 옛 기록의 주소를 쓴다.
fn server_of(env: &Environment) -> (String, String, String) {
    use secrets_core::server::ServerStore;

    if env.server.is_empty() {
        let address = env.address.clone().unwrap_or_default();
        return (address.clone(), String::new(), address);
    }
    match Wiring::get().server_store().load(&env.server) {
        Ok(server) => (server.name, server.kind.id().to_string(), server.address),
        Err(_) => (env.server.clone(), String::new(), String::new()),
    }
}

fn environment_row(project: &str, env: &Environment) -> EnvironmentRow {
    let (server_name, kind, address) = server_of(env);
    EnvironmentRow {
        name: env.name.clone(),
        server: env.server.clone(),
        server_name,
        kind,
        address,
        login: env.login.clone(),
        path: env.path.clone(),
        branch: env.branch.clone(),
        connected_at: env
            .connected_at
            .get(..16)
            .unwrap_or(&env.connected_at)
            .replace('T', " "),
        env_file: env.env_file.clone(),
        server_env_file: env.server_env_file.clone(),
        deploy_script: Wiring::get()
            .deployment()
            .script(project, &env.name)
            .is_ok_and(|s| s.text.is_some()),
    }
}

fn checkout_row(checkout: &Checkout) -> CheckoutRow {
    let bare = |state| CheckoutRow {
        state,
        origin: None,
        branch: None,
        commit: None,
        owner: None,
        group: None,
        ssh_command: None,
    };
    match checkout {
        Checkout::Missing => bare("missing"),
        Checkout::Empty => bare("empty"),
        Checkout::Plain => bare("plain"),
        Checkout::Repository {
            origin,
            branch,
            commit,
            facts,
        } => CheckoutRow {
            state: "repository",
            origin: origin.clone(),
            branch: branch.clone(),
            commit: commit.clone(),
            owner: facts.owner.clone(),
            group: facts.group.clone(),
            ssh_command: facts.ssh_command.clone(),
        },
    }
}

fn path_state(state: PathState) -> &'static str {
    match state {
        PathState::Missing => "missing",
        PathState::EmptyDirectory => "empty",
        PathState::OccupiedDirectory => "occupied",
        PathState::NotDirectory => "not_directory",
    }
}

fn check(path: String) -> PathCheck {
    use secrets_core::project::ProjectStore;

    let wiring = Wiring::get();
    let (state, scan) = wiring.projects().inspect(&path);
    let project = wiring
        .project_store()
        .list()
        .into_iter()
        .filter_map(Result::ok)
        .find(|r: &ProjectRecord| r.path == path)
        .map(|r| r.name);
    PathCheck {
        path: tilde(std::path::Path::new(&path)),
        absolute: path,
        state: path_state(state),
        project,
        scan: scan.map(|s| scan_row(&Ok(s))),
    }
}

fn error(e: ProjectError) -> String {
    e.to_string()
}

#[tauri::command]
pub async fn list_projects() -> ProjectList {
    let (found, errors) = Wiring::get().projects().list();
    ProjectList {
        projects: found.iter().map(row).collect(),
        errors,
    }
}

#[tauri::command]
pub async fn project_detail(name: String) -> Result<ProjectRow, String> {
    let overview = Wiring::get().projects().overview(&name).map_err(error)?;
    Ok(row(&overview))
}

/// 경로를 미리 본다. 아무것도 바꾸지 않는다.
#[tauri::command]
pub async fn inspect_project_path(path: String) -> Result<PathCheck, String> {
    Ok(check(absolute(&path).map_err(error)?))
}

#[tauri::command]
pub async fn create_project(
    app: AppHandle,
    form: NewProjectForm,
) -> Result<CreatedProject, String> {
    let request = NewProject {
        name: form.name,
        group: form.group,
        parent: absolute(&form.parent).map_err(error)?,
        directory: form.directory,
        git: if form.init_git {
            GitStart::Init
        } else {
            GitStart::None
        },
    };
    let projects = Wiring::get().projects();
    let created = projects.create(&request).map_err(error)?;
    let overview = projects.overview(&created.record.name).map_err(error)?;
    let _ = app.emit(UPDATED, ());
    Ok(CreatedProject {
        project: row(&overview),
        incomplete: created.incomplete,
    })
}

/// 이미 있는 디렉토리를 기록한다. 디렉토리는 바꾸지 않는다.
#[tauri::command]
pub async fn register_project(
    app: AppHandle,
    form: RegistrationForm,
) -> Result<ProjectRow, String> {
    let request = Registration {
        name: form.name,
        group: form.group,
        path: absolute(&form.path).map_err(error)?,
    };
    let projects = Wiring::get().projects();
    let record = projects.register(&request).map_err(error)?;
    let overview = projects.overview(&record.name).map_err(error)?;
    let _ = app.emit(UPDATED, ());
    Ok(row(&overview))
}

/// Finder 에서 폴더를 고른다. `~/workspace` 에서 시작하고, 그 밖의 폴더는 받지 않는다.
///
/// 취소하면 `None`. 창이 떠 있는 동안 기다려야 하므로 비동기 명령(작업 스레드)에서만 부른다.
#[tauri::command]
pub async fn pick_project_folder(app: AppHandle, title: String) -> Result<Option<String>, String> {
    let root = workspace_root().map_err(error)?;
    if !root.is_dir() {
        return Err(format!("{}을(를) 찾을 수 없습니다.", tilde(&root)));
    }
    let Some(picked) = app
        .dialog()
        .file()
        .set_title(title)
        .set_directory(&root)
        .blocking_pick_folder()
    else {
        return Ok(None);
    };
    let picked = picked.into_path().map_err(|e| e.to_string())?;
    if !inside(&root, &picked) {
        return Err(format!("{} 안의 폴더만 고를 수 있습니다.", tilde(&root)));
    }
    Ok(Some(tilde(&picked)))
}

/* ── Git 연결 ─────────────────────────────────────────── */

/// 시크릿 저장소에 있는 GitHub 계정들.
fn github_accounts() -> Vec<GithubAccountRow> {
    secrets_local::vault::store::list()
        .into_iter()
        .filter_map(Result::ok)
        .filter(|a| a.provider == secrets_core::account::Provider::Github)
        .map(|a| GithubAccountRow {
            login: if a.identity.name.is_empty() {
                a.slug.clone()
            } else {
                a.identity.name.clone()
            },
            slug: a.slug,
        })
        .collect()
}

/// 시크릿 저장소의 배포 키에 나오는 레포 소유자들.
fn known_owners() -> Vec<String> {
    let mut owners: Vec<String> = Wiring::get()
        .keyring()
        .list()
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|k| k.repo.split_once('/').map(|(owner, _)| owner.to_string()))
        .collect();
    owners.sort();
    owners.dedup();
    owners
}

/// 연결하기 전에 보여 줄 것. 아무것도 바꾸지 않는다.
#[tauri::command]
pub async fn git_plan(name: String) -> Result<GitPlanRow, String> {
    let wiring = Wiring::get();
    let plan = wiring.git_link().plan(&name).map_err(error)?;
    let overview = wiring.projects().overview(&name).map_err(error)?;
    let tracked = overview
        .scan
        .as_ref()
        .map(|s| {
            s.env_view()
                .into_iter()
                .filter(|f| f.exposed() && f.tracked)
                .map(|f| f.name)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let ignorable = plan
        .exposed
        .iter()
        .filter(|n| !tracked.contains(n))
        .cloned()
        .collect();

    let current = plan.current_key.clone();
    let current_key_in_vault = plan
        .keys
        .iter()
        .any(|k| Some(&k.private_key) == current.as_ref());
    let (git, origin) = match &plan.state {
        GitState::Absent => ("absent", None),
        GitState::Local { .. } => ("local", None),
        GitState::Remote { origin, .. } => ("remote", Some(origin.clone())),
    };
    Ok(GitPlanRow {
        git,
        origin,
        repo: plan.repo.as_ref().map(RepoRef::slug),
        keys: plan
            .keys
            .iter()
            .map(|k| RepoKeyRow {
                purpose: k.purpose.clone(),
                account: k.account.clone(),
                write: k.write,
                usable: k.usable,
                in_use: Some(&k.private_key) == current.as_ref(),
            })
            .collect(),
        current_key: current.map(|p| tilde(std::path::Path::new(&p))),
        current_key_in_vault,
        ignorable,
        tracked,
        accounts: github_accounts(),
        owners: known_owners(),
    })
}

/// 값이 원격에 올라갈 수 있는 환경 변수 파일을 `.gitignore` 에 더한다.
#[tauri::command]
pub async fn ignore_env_files(app: AppHandle, name: String) -> Result<Vec<String>, String> {
    let added = Wiring::get()
        .projects()
        .ignore_exposed(&name)
        .map_err(error)?;
    let _ = app.emit(UPDATED, ());
    Ok(added)
}

fn git_request(form: &GitConnectForm) -> Result<GitRequest, String> {
    let remote = match form.remote.kind.as_str() {
        "current" => RemoteChoice::Current,
        "existing" => RemoteChoice::Existing(form.remote.url.clone()),
        "create" => {
            let slug = format!("{}/{}", form.remote.owner.trim(), form.remote.name.trim());
            let repo = RepoRef::parse(&slug)
                .ok_or_else(|| format!("{slug}은(는) 레포 이름으로 쓸 수 없습니다."))?;
            let visibility = if form.remote.private {
                Visibility::Private
            } else {
                Visibility::Public
            };
            RemoteChoice::Create { repo, visibility }
        }
        other => return Err(format!("알 수 없는 원격 선택입니다: {other}")),
    };
    // 키는 자격 증명 화면에서 발급한 것만 고른다. none 이면 core.sshCommand 를 그대로 둔다.
    let key = match form.key.kind.as_str() {
        "stored" => Some(form.key.purpose.trim().to_string()),
        "none" => None,
        other => return Err(format!("알 수 없는 키 선택입니다: {other}")),
    };
    Ok(GitRequest {
        account: form.account.clone(),
        remote,
        key,
    })
}

/// 원격과 레포 전용 키를 잇는다. 단계마다 작업 로그에 흘린다.
#[tauri::command]
pub async fn connect_git(app: AppHandle, form: GitConnectForm) -> Result<LinkedRow, String> {
    let request = git_request(&form)?;
    let job = next_job_id();
    let label = format!("{} Git 연결", form.project);
    let _ = app.emit(
        "cli:start",
        Started {
            job: job.clone(),
            command: label.clone(),
        },
    );
    let panel = JobPanel {
        app: app.clone(),
        job: job.clone(),
    };

    let result = Wiring::get()
        .git_link()
        .connect(&form.project, &request, &panel)
        .map(|linked| LinkedRow {
            repo: linked.repo.slug(),
            purpose: linked.key.map(|k| k.purpose),
            created_repository: linked.created_repository,
            unreachable: linked.unreachable,
        })
        .map_err(error);

    let _ = app.emit(
        "cli:end",
        Ended {
            job,
            ok: matches!(&result, Ok(row) if row.unreachable.is_none()),
            message: match &result {
                Ok(row) => match &row.unreachable {
                    None => label,
                    Some(why) => format!("{label} — 설정은 끝났지만 접속 확인 실패: {why}"),
                },
                Err(e) => e.clone(),
            },
        },
    );
    // 원격 · 키가 바뀌었을 수 있다. 두 화면 모두 다시 읽게 한다.
    let _ = app.emit(UPDATED, ());
    let _ = app.emit("keys:updated", ());
    result
}

/* ── 서버 연결 ────────────────────────────────────────── */

/// 프로젝트 환경들이 어느 서버 계정을 쓰는지 (`서버/계정` → `프로젝트/환경`).
fn usage() -> Vec<(String, String, String)> {
    use secrets_core::project::ProjectStore;

    Wiring::get()
        .project_store()
        .list()
        .into_iter()
        .filter_map(Result::ok)
        .flat_map(|r| {
            r.environments
                .into_iter()
                .map(move |e| (e.server, e.login, format!("{}/{}", r.name, e.name)))
        })
        .collect()
}

/// 서버 연결 창이 보여 줄 것 — 이 프로젝트의 레포와, 등록된 서버마다 그 위의 계정.
#[tauri::command]
pub async fn server_plan(project: String) -> ServerPlanRow {
    let link = Wiring::get().server_link();
    let (repo, problem) = match link.repo_of(&project) {
        Ok(repo) => (Some(repo), None),
        Err(e) => (None, Some(e.to_string())),
    };
    let used = usage();
    let servers = link
        .servers()
        .into_iter()
        .map(|i| ServerChoiceRow {
            accounts: i
                .accounts
                .iter()
                .map(|seat| SeatRow {
                    login: seat.login.clone(),
                    admin: seat.admin,
                    verified: seat.verified,
                    used_by: used
                        .iter()
                        .filter(|(server, login, _)| *server == seat.server && *login == seat.login)
                        .map(|(_, _, env)| env.clone())
                        .collect(),
                })
                .collect(),
            group: group_of(&i.server),
            server: i.server,
            name: i.name,
            address: i.address,
            kind: i.kind,
        })
        .collect();
    ServerPlanRow {
        repo_name: repo.as_ref().map(|r| r.name().to_string()),
        repo: repo.as_ref().map(RepoRef::slug),
        problem,
        servers,
    }
}

/// 오래 걸리는 일을 작업 로그에 흘린다. `changed` 면 끝난 뒤 목록을 다시 읽게 한다.
fn job<T>(
    app: &AppHandle,
    label: String,
    changed: bool,
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
    if changed {
        let _ = app.emit(UPDATED, ());
    }
    result
}

/// 고른 서버 계정을 환경으로 잇는다. 서버는 읽기만 한다.
#[tauri::command]
pub async fn attach_server(app: AppHandle, form: ServerForm) -> Result<AttachedRow, String> {
    let request = ServerRequest {
        environment: form.environment.clone(),
        server: form.server.clone(),
        login: form.login.clone(),
        path: form.path.clone(),
        branch: form.branch.clone(),
    };
    let label = format!("{} 서버 연결 · {}", form.project, form.environment.trim());
    job(&app, label, true, |panel| {
        Wiring::get()
            .server_link()
            .attach(&form.project, &request, panel)
            .map(|attached| AttachedRow {
                environment: environment_row(&form.project, &attached.environment),
                checkout: checkout_row(&attached.checkout),
            })
            .map_err(error)
    })
}

/// 붙은 환경의 배포 경로에 지금 무엇이 있는지 읽는다. 아무것도 바꾸지 않는다.
#[tauri::command]
pub async fn check_environment(
    app: AppHandle,
    project: String,
    environment: String,
) -> Result<CheckoutRow, String> {
    let label = format!("{project} · {environment} 서버 확인");
    job(&app, label, false, |panel| {
        Wiring::get()
            .server_link()
            .check(&project, &environment, panel)
            .map(|c| checkout_row(&c))
            .map_err(error)
    })
}

/// 입력한 레포의 저장된 키. 아직 origin 이 아닌 레포를 고를 때 쓴다. 읽기만 한다.
#[tauri::command]
pub async fn repo_keys(repo: String) -> Result<Vec<RepoKeyRow>, String> {
    let target = RepoRef::parse(&repo)
        .ok_or_else(|| format!("{repo}을(를) GitHub 레포로 읽지 못했습니다."))?;
    Ok(Wiring::get()
        .git_link()
        .keys_for(&target)
        .into_iter()
        .map(|k| RepoKeyRow {
            purpose: k.purpose,
            account: k.account,
            write: k.write,
            usable: k.usable,
            in_use: false,
        })
        .collect())
}

/* ── 코드 받기 ────────────────────────────────────────── */

fn key_row(k: &secrets_core::project::RepoKey) -> RepoKeyRow {
    RepoKeyRow {
        purpose: k.purpose.clone(),
        account: k.account.clone(),
        write: k.write,
        usable: k.usable,
        in_use: false,
    }
}

/// 코드를 받기 전에 보여 줄 것 — 어느 서버의 어디로, 어떤 키로. 서버는 읽지 않는다.
#[tauri::command]
pub async fn pull_plan(project: String, environment: String) -> Result<PullPlanRow, String> {
    use secrets_core::project::ProjectStore;

    let wiring = Wiring::get();
    let record = wiring.project_store().load(&project).map_err(error)?;
    let env = record
        .environments
        .iter()
        .find(|e| e.name == environment)
        .ok_or_else(|| format!("환경 {environment}을(를) 찾을 수 없습니다."))?;
    let (repo, keys, problem) = match wiring.code_pull().keys_of(&project) {
        Ok((repo, keys)) => (Some(repo.slug()), keys.iter().map(key_row).collect(), None),
        Err(e) => (None, Vec::new(), Some(e.to_string())),
    };
    Ok(PullPlanRow {
        environment: environment_row(&project, env),
        repo,
        keys,
        problem,
    })
}

/// 서버의 배포 경로로 코드를 받는다. 비어 있을 때만 받고, 받은 뒤 다시 읽어 확인한다.
#[tauri::command]
pub async fn pull_code(app: AppHandle, form: PullForm) -> Result<PulledRow, String> {
    let purpose = form.key.trim().to_string();
    let label = format!("{} · {} 코드 받기", form.project, form.environment);
    job(&app, label, true, |panel| {
        Wiring::get()
            .code_pull()
            .pull(&form.project, &form.environment, &purpose, panel)
            .map(|pulled| PulledRow {
                checkout: checkout_row(&pulled.checkout),
                already: pulled.already,
            })
            .map_err(error)
    })
}

/* ── 환경 변수 ────────────────────────────────────────── */

fn comparison_row(found: &EnvComparison) -> EnvComparisonRow {
    let (state, local_only, server_only, changed) = match &found.state {
        EnvState::NoDirectory => ("no_directory", vec![], vec![], vec![]),
        EnvState::ServerMissing => ("server_missing", vec![], vec![], vec![]),
        EnvState::Same => ("same", vec![], vec![], vec![]),
        EnvState::Differ {
            local_only,
            server_only,
            changed,
        } => (
            "differ",
            local_only.clone(),
            server_only.clone(),
            changed.clone(),
        ),
    };
    EnvComparisonRow {
        local_file: found.local_file.clone(),
        server_file: found.server_file.clone(),
        state,
        local_only,
        server_only,
        changed,
        mode: found.mode.clone(),
        owner: found.owner.clone(),
        tracking: found.tracking.map(|t| match t {
            RepoTracking::Ignored => "ignored",
            RepoTracking::Unignored => "unignored",
            RepoTracking::Tracked => "tracked",
            RepoTracking::NoRepository => "none",
        }),
    }
}

/// 이 환경에 올릴 로컬 파일과 서버에 둘 이름을 정한다. 기록만 바꾼다.
#[tauri::command]
pub async fn choose_env_file(
    app: AppHandle,
    project: String,
    environment: String,
    file: Option<String>,
    server_file: String,
) -> Result<(), String> {
    Wiring::get()
        .env_sync()
        .choose(&project, &environment, file.as_deref(), &server_file)
        .map_err(error)?;
    let _ = app.emit(UPDATED, ());
    Ok(())
}

/// 로컬 파일과 서버 파일을 해시로 비교한다. 아무것도 바꾸지 않는다.
#[tauri::command]
pub async fn compare_env(
    app: AppHandle,
    project: String,
    environment: String,
) -> Result<EnvComparisonRow, String> {
    let label = format!("{project} · {environment} 환경 변수 비교");
    job(&app, label, false, |panel| {
        Wiring::get()
            .env_sync()
            .compare(&project, &environment, panel)
            .map(|c| comparison_row(&c))
            .map_err(error)
    })
}

/// 로컬 파일을 서버 파일로 올린다.
#[tauri::command]
pub async fn push_env(
    app: AppHandle,
    project: String,
    environment: String,
) -> Result<EnvComparisonRow, String> {
    let label = format!("{project} · {environment} 환경 변수 반영");
    job(&app, label, false, |panel| {
        Wiring::get()
            .env_sync()
            .push(&project, &environment, panel)
            .map(|c| comparison_row(&c))
            .map_err(error)
    })
}

/* ── 등록한 뒤의 수정 · 제거 ──────────────────────────── */

/// 프로젝트의 이름 · 그룹 · 경로를 바꾼다. 기록만 바꾼다.
#[tauri::command]
pub async fn update_project(app: AppHandle, form: ProjectEditForm) -> Result<ProjectRow, String> {
    let path = absolute(&form.path).map_err(error)?;
    let edit = ProjectEdit {
        name: form.name,
        group: form.group,
        path,
    };
    let wiring = Wiring::get();
    let record = wiring
        .project_editor()
        .update(&form.project, &edit)
        .map_err(error)?;
    let overview = wiring.projects().overview(&record.name).map_err(error)?;
    let _ = app.emit(UPDATED, ());
    Ok(row(&overview))
}

/// 등록을 해제한다. 기록을 보관소로 옮긴다. 디렉토리 · 서버 · 레포 · 키는 그대로다.
#[tauri::command]
pub async fn unregister_project(app: AppHandle, project: String) -> Result<String, String> {
    let kept = Wiring::get()
        .project_editor()
        .unregister(&project)
        .map_err(error)?;
    let _ = app.emit(UPDATED, ());
    Ok(tilde_str(&kept))
}

/// 환경의 이름 · 서버 계정 · 배포 경로 · 브랜치를 바꾼다. 서버 쪽이 바뀌면 서버를 다시 읽는다.
#[tauri::command]
pub async fn update_environment(
    app: AppHandle,
    form: EnvironmentEditForm,
) -> Result<EditedEnvironmentRow, String> {
    let edit = EnvironmentEdit {
        name: form.name.clone(),
        server: form.server.clone(),
        login: form.login.clone(),
        path: form.path.clone(),
        branch: form.branch.clone(),
    };
    let label = format!("{} · {} 환경 편집", form.project, form.environment);
    job(&app, label, true, |panel| {
        Wiring::get()
            .project_editor()
            .update_environment(&form.project, &form.environment, &edit, panel)
            .map(|done| EditedEnvironmentRow {
                environment: environment_row(&form.project, &done.environment),
                checkout: done.checkout.as_ref().map(checkout_row),
            })
            .map_err(error)
    })
}

/// 환경을 뺀다. 기록과 배포 스크립트를 보관소로 옮긴다. 서버의 코드와 환경 변수 파일은 그대로다.
#[tauri::command]
pub async fn remove_environment(
    app: AppHandle,
    project: String,
    environment: String,
) -> Result<String, String> {
    let kept = Wiring::get()
        .project_editor()
        .remove_environment(&project, &environment)
        .map_err(error)?;
    let _ = app.emit(UPDATED, ());
    Ok(tilde_str(&kept))
}

/* ── 자격 증명 연결 ───────────────────────────────────── */

/// 소비처 한 곳이 이 프로젝트의 어디인가.
struct Placed {
    /// 서버 쪽이면 `~/.ssh/config` 의 호스트 이름. 이 맥이면 없다.
    host: Option<String>,
    /// 프로젝트 안(서버면 배포 경로 안)의 경로.
    file: String,
    environment: Option<String>,
}

/// 소비처 `host:file` 을 이 프로젝트에 대어 본다.
///
/// 이 맥의 파일은 프로젝트 디렉토리 안이면, 서버의 파일은 그 호스트가 환경의 서버 주소를 가리키고
/// 배포 경로 안이면 이 프로젝트의 것이다. 이 맥의 파일은 환경 변수 파일로 고른 환경에 붙인다.
fn place_in(
    record: &ProjectRecord,
    hosts: &[secrets_local::keys::hosts::Host],
    host: &str,
    file: &str,
) -> Option<Placed> {
    let under = |root: &str, path: &str| {
        path.strip_prefix(&format!("{}/", root.trim_end_matches('/')))
            .map(str::to_string)
    };
    if host == secrets_core::aws::iam::LOCAL_HOST {
        let file = under(&record.path, &absolute(file).ok()?)?;
        let environment = record
            .environments
            .iter()
            .find(|e| e.env_file.as_deref() == Some(file.as_str()))
            .map(|e| e.name.clone());
        return Some(Placed {
            host: None,
            file,
            environment,
        });
    }
    let address = hosts
        .iter()
        .find(|h| h.alias == host)
        .map(|h| h.address.clone().unwrap_or_else(|| h.alias.clone()))?;
    record
        .environments
        .iter()
        .filter(|e| server_of(e).2 == address)
        .find_map(|e| {
            Some(Placed {
                host: Some(host.to_string()),
                file: under(&e.path, file)?,
                environment: Some(e.name.clone()),
            })
        })
}

/// 이 프로젝트의 파일을 가리키는 자격 증명과, 연결할 때 고를 수 있는 자격 증명.
///
/// 연결은 자격 증명 쪽의 소비처 기록이다(`add_iam_consumer` · `add_etc_consumer`). 프로젝트는
/// 자격 증명을 만들지 않고, 이미 있는 것을 어디에 넣었는지 기록만 한다. 보여 줄 때는 이 맥의
/// 파일과 연결된 서버의 파일을 함께 보인다.
#[tauri::command]
pub async fn project_credentials(project: String) -> Result<ProjectCredentialsRow, String> {
    use secrets_core::etc::EtcVault;
    use secrets_core::project::ProjectStore;

    let wiring = Wiring::get();
    let record = wiring.project_store().load(&project).map_err(error)?;
    let hosts = secrets_local::keys::hosts::known();

    let mut linked = Vec::new();
    let mut iams = Vec::new();
    for user in wiring.issuer().list().into_iter().flatten() {
        for c in &user.consumers {
            if let Some(at) = place_in(&record, &hosts, &c.host, &c.file) {
                linked.push(LinkedCredentialRow {
                    kind: "iam",
                    name: user.name.clone(),
                    purpose: user.purpose.clone(),
                    detail: format!("{} / {}", c.id_variable, c.secret_variable),
                    variable: Some(c.id_variable.clone()),
                    host: at.host,
                    environment: at.environment,
                    file: at.file,
                });
            }
        }
        if user.cleanup.is_none() {
            iams.push(CredentialChoiceRow {
                owner: user.account.clone(),
                name: user.name.clone(),
                purpose: user.purpose.clone(),
            });
        }
    }
    let mut etcs = Vec::new();
    for item in wiring.etc_vault().list().into_iter().flatten() {
        for c in &item.consumers {
            if let Some(at) = place_in(&record, &hosts, &c.host, &c.file) {
                linked.push(LinkedCredentialRow {
                    kind: "etc",
                    name: format!("{}/{}", item.project, item.name),
                    purpose: item.purpose.clone(),
                    detail: item.kind.clone(),
                    variable: None,
                    host: at.host,
                    environment: at.environment,
                    file: at.file,
                });
            }
        }
        etcs.push(CredentialChoiceRow {
            owner: item.project.clone(),
            name: item.name.clone(),
            purpose: item.purpose.clone(),
        });
    }
    linked.sort_by(|a, b| (&a.host, &a.file, &a.name).cmp(&(&b.host, &b.file, &b.name)));
    iams.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(ProjectCredentialsRow { linked, iams, etcs })
}

/// 자격 증명마다 그것을 쓰는 프로젝트 · 환경. 자격 증명 화면이 "어디에 쓰이나"를 보인다.
///
/// - 배포 키(`github:<레포>`): 그 레포를 쓰는 프로젝트. 로컬 git 이 금고의 키를 가리키면
///   그 용도를 적는다. 서버에 둔 키는 이 도구가 확인하지 않으므로 환경만 적는다.
/// - IAM(`iam:<계정>/<이름>`) · 기타(`etc:<그룹>/<이름>`): 사용 위치 기록이 프로젝트 안
///   (서버면 배포 경로 안)이면 그 프로젝트.
/// - 서버 키(`pem:<리전>/<키페어>`): 그 pem 으로 들어가는 계정이 있는 서버에 연결된 환경.
///
/// 읽기만 한다.
#[tauri::command]
pub async fn credential_usage() -> Vec<CredentialUseRow> {
    use secrets_core::etc::EtcVault;

    let wiring = Wiring::get();
    let hosts = secrets_local::keys::hosts::known();
    let (overviews, _) = wiring.projects().list();
    let users: Vec<_> = wiring.issuer().list().into_iter().flatten().collect();
    let items: Vec<_> = wiring.etc_vault().list().into_iter().flatten().collect();
    let mut uses = Vec::new();

    for overview in &overviews {
        let record = &overview.record;
        let use_of = |credential: String, environment: Option<String>, host: Option<String>, file: Option<String>, purpose: Option<String>| CredentialUseRow {
            credential,
            project: record.name.clone(),
            environment,
            host,
            file,
            purpose,
        };
        if let Ok(scan) = &overview.scan
            && let GitState::Remote { origin, .. } = &scan.git
            && let Some(repo) = RepoRef::parse(origin)
        {
            let id = format!("github:{}", repo.slug().to_lowercase());
            // ~/.secrets/keys/github/repo/<소유자>/<레포>/<용도>/key
            let purpose = scan
                .ssh_key
                .as_deref()
                .filter(|key| key.contains("/.secrets/keys/github/"))
                .and_then(|key| key.rsplit('/').nth(1))
                .map(str::to_string);
            uses.push(use_of(id.clone(), None, None, None, purpose));
            for env in &record.environments {
                uses.push(use_of(id.clone(), Some(env.name.clone()), Some(server_of(env).0), None, None));
            }
        }
        for env in &record.environments {
            for pem in pems_of(&env.server) {
                uses.push(use_of(pem, Some(env.name.clone()), Some(server_of(env).0), None, Some(env.login.clone())));
            }
        }
        for user in &users {
            for c in &user.consumers {
                if let Some(at) = place_in(record, &hosts, &c.host, &c.file) {
                    uses.push(use_of(format!("iam:{}", user.at().slug()), at.environment, at.host, Some(at.file), None));
                }
            }
        }
        for item in &items {
            for c in &item.consumers {
                if let Some(at) = place_in(record, &hosts, &c.host, &c.file) {
                    uses.push(use_of(format!("etc:{}", item.at().slug()), at.environment, at.host, Some(at.file), None));
                }
            }
        }
    }
    uses
}

/// 서버의 그룹. 서버 기록을 읽지 못하면 비어 있다.
fn group_of(server: &str) -> String {
    use secrets_core::server::ServerStore;

    Wiring::get()
        .server_store()
        .load(server)
        .map(|s| s.group)
        .unwrap_or_default()
}

/// 그 서버의 계정이 들어갈 때 쓰는 pem 들 (`pem:<리전>/<키페어>`).
fn pems_of(server: &str) -> Vec<String> {
    use secrets_core::server::{AccountKey, ServerStore};

    let Ok(server) = Wiring::get().server_store().load(server) else {
        return Vec::new();
    };
    let Some(aws) = &server.aws else {
        return Vec::new();
    };
    let mut pems: Vec<String> = server
        .accounts
        .iter()
        .filter_map(|a| match &a.key {
            AccountKey::Pem { keypair } => Some(format!("pem:{}/{keypair}", aws.region)),
            _ => None,
        })
        .collect();
    pems.dedup();
    pems
}

/* ── 배포 ─────────────────────────────────────────────── */

fn revision_row(r: &Revision) -> RevisionRow {
    RevisionRow {
        sha: r.sha.chars().take(7).collect(),
        subject: r.subject.clone(),
    }
}

fn note_row(note: &CodeNote, branch: &str) -> NoteRow {
    let warn = |text: String| NoteRow { tone: "warn", text };
    let info = |text: String| NoteRow { tone: "info", text };
    match note {
        CodeNote::FetchFailed(e) => warn(format!(
            "원격을 가져오지 못해 마지막으로 받아 둔 정보로 비교했습니다. {e}"
        )),
        CodeNote::RemoteMissing => warn(format!(
            "원격에 {branch} 브랜치가 없습니다. 스크립트가 받을 코드가 없습니다."
        )),
        CodeNote::LocalMissing => info(format!("로컬에 {branch} 브랜치가 없습니다.")),
        CodeNote::OtherBranch(current) => info(format!(
            "로컬은 지금 {current} 브랜치에 있습니다. 배포되는 것은 {branch}입니다."
        )),
        CodeNote::Uncommitted(n) => warn(format!("커밋하지 않은 변경 {n}개는 배포되지 않습니다.")),
        CodeNote::LocalAhead(n) => warn(format!(
            "로컬 {branch}에만 있는 커밋 {n}개 — 푸시하지 않아 배포되지 않습니다."
        )),
        CodeNote::LocalBehind(n) => warn(format!(
            "원격에만 있는 커밋 {n}개 — 로컬에서 받지 않은 커밋이 배포됩니다."
        )),
        CodeNote::ServerUnreadable(e) => warn(format!("서버를 읽지 못했습니다. {e}")),
        CodeNote::ServerNotRepository => warn("서버의 배포 경로가 git 저장소가 아닙니다.".into()),
        CodeNote::ServerOtherBranch(b) => warn(format!("서버는 {b} 브랜치에 있습니다.")),
        CodeNote::ServerAhead(n) => warn(format!(
            "서버에만 있는 커밋 {n}개 — 원격으로 fast-forward 하는 스크립트는 실패합니다."
        )),
        CodeNote::ServerUnknownCommit(sha) => warn(format!(
            "서버의 커밋 {sha}을(를) 로컬에서 찾지 못해 비교하지 못했습니다."
        )),
        CodeNote::EnvNotChosen => warn("환경 변수 파일을 고르지 않아 비교하지 않았습니다.".into()),
    }
}

/// 원격과 견준 관계 한 마디.
fn relation(ahead: u32, behind: u32, known: bool) -> String {
    match (known, ahead, behind) {
        (false, _, _) => "알 수 없음".into(),
        (true, 0, 0) => "같음".into(),
        (true, a, 0) => format!("{a}개 앞섬"),
        (true, 0, b) => format!("{b}개 뒤"),
        (true, a, b) => format!("{a}개 앞섬 · {b}개 뒤"),
    }
}

fn blocker_text(blocker: &Blocker) -> String {
    match blocker {
        Blocker::NoScript => "배포 스크립트가 없습니다.".into(),
        Blocker::EnvDiffers => {
            "환경 변수 파일이 로컬과 서버에서 다릅니다. [환경 변수]에서 반영하세요.".into()
        }
        Blocker::EnvUnchecked(e) => format!("환경 변수 파일을 비교하지 못했습니다. {e}"),
    }
}

/// 배포 전에 볼 것. 로컬 레포의 원격 추적 브랜치를 갱신하는 것 말고는 아무것도 바꾸지 않는다.
#[tauri::command]
pub async fn deploy_plan(
    app: AppHandle,
    project: String,
    environment: String,
) -> Result<DeployPlanRow, String> {
    let label = format!("{project} · {environment} 배포 확인");
    job(&app, label, false, |panel| {
        let wiring = Wiring::get();
        let env = wiring.env_sync();
        let plan = wiring
            .deployer(&env)
            .plan(&project, &environment, panel)
            .map_err(error)?;
        let pick = |f: fn(&CodeNote) -> Option<u32>| plan.notes.iter().find_map(f).unwrap_or(0);
        let local_relation = relation(
            pick(|n| {
                if let CodeNote::LocalAhead(k) = n {
                    Some(*k)
                } else {
                    None
                }
            }),
            pick(|n| {
                if let CodeNote::LocalBehind(k) = n {
                    Some(*k)
                } else {
                    None
                }
            }),
            plan.local.is_some() && plan.remote.is_some(),
        );
        let server_relation = relation(
            pick(|n| {
                if let CodeNote::ServerAhead(k) = n {
                    Some(*k)
                } else {
                    None
                }
            }),
            plan.incoming.unwrap_or(0),
            plan.server.is_some() && plan.incoming.is_some(),
        );
        Ok(DeployPlanRow {
            local_relation,
            server_relation,
            local: plan.local.as_ref().map(revision_row),
            remote: plan.remote.as_ref().map(revision_row),
            server: plan.server.as_ref().map(revision_row),
            incoming: plan.incoming,
            notes: plan
                .notes
                .iter()
                .map(|n| note_row(n, &plan.branch))
                .collect(),
            same: plan.same,
            env: plan.env.as_ref().map(comparison_row),
            script: plan.script.as_deref().map(tilde_str),
            blockers: plan.blockers.iter().map(blocker_text).collect(),
            branch: plan.branch,
        })
    })
}

/// 배포 스크립트를 서버에서 돌린다. 출력은 작업 로그로 흐른다.
#[tauri::command]
pub async fn run_deploy(
    app: AppHandle,
    project: String,
    environment: String,
) -> Result<DeployedRow, String> {
    let label = format!("{project} · {environment} 배포");
    let result = job(&app, label, false, |panel| {
        let wiring = Wiring::get();
        let env = wiring.env_sync();
        let done = wiring
            .deployer(&env)
            .run(&project, &environment, panel)
            .map_err(error)?;
        Ok(DeployedRow {
            before: done.before.as_ref().map(revision_row),
            after: done.after.as_ref().map(revision_row),
        })
    });
    let _ = app.emit(UPDATED, ());
    result
}

/* ── 배포 스크립트 ─────────────────────────────────────── */

fn tilde_str(path: &str) -> String {
    tilde(std::path::Path::new(path))
}

/// 이 환경의 배포 스크립트와 스크립트가 받는 환경 변수. 읽기만 한다.
#[tauri::command]
pub async fn deploy_script(
    project: String,
    environment: String,
) -> Result<DeployScriptRow, String> {
    let found = Wiring::get()
        .deployment()
        .script(&project, &environment)
        .map_err(error)?;
    Ok(DeployScriptRow {
        path: tilde_str(&found.path),
        text: found.text,
        variables: secrets_core::project::deploy::SCRIPT_VARIABLES.to_vec(),
    })
}

/// 이 환경의 배포 스크립트를 쓴다. 이전 스크립트는 보관소로 옮긴다.
#[tauri::command]
pub async fn save_deploy_script(
    app: AppHandle,
    project: String,
    environment: String,
    text: String,
) -> Result<SavedScriptRow, String> {
    let saved = Wiring::get()
        .deployment()
        .save_script(&project, &environment, &text)
        .map_err(error)?;
    let _ = app.emit(UPDATED, ());
    Ok(SavedScriptRow {
        path: tilde_str(&saved.path),
        unchanged: saved.unchanged,
        archived: saved.archived.as_deref().map(tilde_str),
    })
}

