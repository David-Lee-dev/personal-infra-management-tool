//! 프로젝트 — 목록 · 경로 미리 보기 · 새로 만들기 · 기존 디렉토리 등록.
//!
//! 사용자의 작업 공간에 쓰는 일은 새 디렉토리 만들기와 `git init` 뿐이다. 등록은 기록만
//! 남긴다. 목록과 상세는 디렉토리를 스캔하므로(git 질의) 비동기로 돌려 화면을 막지 않는다.

use secrets_core::key::RepoRef;
use secrets_core::project::{
    Checkout, EnvFileRole, Environment, GitRequest, GitStart, GitState, KeyChoice, LocalScan,
    NewProject, Origin, Overview, PathState, ProjectError, ProjectRecord, Registration,
    RemoteChoice, ServerRequest, StageState, Stages, Visibility,
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
        environments: record.environments.iter().map(environment_row).collect(),
    }
}

fn environment_row(env: &Environment) -> EnvironmentRow {
    EnvironmentRow {
        name: env.name.clone(),
        machine: env.machine.clone(),
        instance: env.instance.clone(),
        instance_name: env.instance_name.clone(),
        address: env.address.clone(),
        login: env.login.clone(),
        path: env.path.clone(),
        connected_at: env
            .connected_at
            .get(..16)
            .unwrap_or(&env.connected_at)
            .replace('T', " "),
    }
}

fn checkout_row(checkout: &Checkout) -> CheckoutRow {
    match checkout {
        Checkout::Missing => CheckoutRow {
            state: "missing",
            origin: None,
            branch: None,
            commit: None,
        },
        Checkout::Empty => CheckoutRow {
            state: "empty",
            origin: None,
            branch: None,
            commit: None,
        },
        Checkout::Plain => CheckoutRow {
            state: "plain",
            origin: None,
            branch: None,
            commit: None,
        },
        Checkout::Repository {
            origin,
            branch,
            commit,
        } => CheckoutRow {
            state: "repository",
            origin: origin.clone(),
            branch: branch.clone(),
            commit: commit.clone(),
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
    let purpose = form.key.purpose.trim().to_string();
    let key = match form.key.kind.as_str() {
        "stored" => KeyChoice::Stored { purpose },
        "issue" => KeyChoice::Issue { purpose },
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
            purpose: linked.key.purpose,
            created_repository: linked.created_repository,
            issued_key: linked.issued_key,
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

/// 서버 연결 창이 보여 줄 것 — 배포 경로(규칙으로 정해진다)와 인스턴스별 배포 계정.
///
/// 관리 계정은 배포에 쓰지 않으므로 목록에 넣지 않고 개수만 알려 준다.
#[tauri::command]
pub async fn server_plan(project: String) -> ServerPlanRow {
    use secrets_core::project::ProjectStore;

    let wiring = Wiring::get();
    let link = wiring.server_link();
    let (deploy_path, problem) = match link.deploy_path_of(&project) {
        Ok(path) => (Some(path), None),
        Err(e) => (None, Some(e.to_string())),
    };
    let records: Vec<ProjectRecord> = wiring.project_store().list().into_iter().filter_map(Result::ok).collect();
    let used_by = |instance: &str, login: &str| -> Vec<String> {
        records
            .iter()
            .flat_map(|r| {
                r.environments
                    .iter()
                    .filter(|e| e.instance == instance && e.login == login)
                    .map(move |e| format!("{}/{}", r.name, e.name))
            })
            .collect()
    };

    let mut instances: Vec<InstanceRow> = Vec::new();
    for seat in link.seats() {
        let at = match instances.iter().position(|i| i.instance == seat.instance) {
            Some(at) => at,
            None => {
                instances.push(InstanceRow {
                    instance: seat.instance.clone(),
                    name: seat.instance_name.clone(),
                    address: seat.address.clone(),
                    machine: seat.machine.clone(),
                    accounts: Vec::new(),
                    admins: 0,
                });
                instances.len() - 1
            }
        };
        if seat.admin {
            instances[at].admins += 1;
            continue;
        }
        instances[at].accounts.push(SeatRow {
            r#ref: seat.slug(),
            used_by: used_by(&seat.instance, &seat.login),
            login: seat.login,
            verified: seat.verified,
        });
    }
    instances.sort_by(|a, b| (&a.name, &a.instance).cmp(&(&b.name, &b.instance)));
    ServerPlanRow {
        deploy_path,
        problem,
        instances,
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

/// 확인된 배포 계정을 환경으로 잇는다. 서버는 읽기만 한다.
#[tauri::command]
pub async fn attach_server(app: AppHandle, form: ServerForm) -> Result<AttachedRow, String> {
    let request = ServerRequest {
        environment: form.environment.clone(),
        seat: form.seat.clone(),
    };
    let label = format!("{} 서버 연결 · {}", form.project, form.environment.trim());
    job(&app, label, true, |panel| {
        Wiring::get()
            .server_link()
            .attach(&form.project, &request, panel)
            .map(|attached| AttachedRow {
                environment: environment_row(&attached.environment),
                checkout: checkout_row(&attached.checkout),
            })
            .map_err(error)
    })
}

/// 붙은 환경의 배포 경로를 지금 읽는다. 아무것도 바꾸지 않는다.
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
    let target = RepoRef::parse(&repo).ok_or_else(|| format!("{repo}을(를) GitHub 레포로 읽지 못했습니다."))?;
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
