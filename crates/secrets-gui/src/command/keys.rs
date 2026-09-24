//! 배포 키 목록·생성·재발급·삭제·내보내기.

use secrets_core::key::{DeployKey, KeyRef, KeyState, RepoRef};
use secrets_local::keys::paths;
use tauri::{AppHandle, Emitter};

use crate::dto::*;
use crate::progress::*;
use crate::wiring::Wiring;

/// 화면이 준 좌표를 키 자리로 읽는다.
fn locate(repo: &str, purpose: &str) -> Result<KeyRef, String> {
    let target = RepoRef::parse(repo).ok_or_else(|| format!("리포지토리를 읽지 못했습니다: {repo}"))?;
    KeyRef::new(target, purpose).ok_or_else(|| format!("용도로 쓸 수 없습니다: {purpose}"))
}

/// 홈 아래 경로는 `~` 로 줄인다. 화면 폭이 한정돼 있고, 되풀이되는 앞부분보다
/// 뒤쪽이 실제로 구분되는 자리다.
pub fn tilde(path: &std::path::Path) -> String {
    let text = path.display().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() => match text.strip_prefix(&home) {
            Some(rest) => format!("~{rest}"),
            None => text,
        },
        _ => text,
    }
}

fn row(key: &DeployKey) -> KeyRow {
    let at = key.at();
    KeyRow {
        r#ref: at.as_ref().map(KeyRef::slug).unwrap_or_default(),
        domain: "github",
        purpose: key.purpose.clone(),
        repo: key.repo.clone(),
        account: key.account.clone(),
        write: key.write,
        algorithm: key.algorithm.clone(),
        fingerprint: key.fingerprint.clone(),
        path: at.as_ref().map(|a| tilde(&paths::dir_of(a))).unwrap_or_default(),
        created_at: key.created_at.clone(),
        state: match key.state {
            KeyState::Local => "local",
            KeyState::Registered => "registered",
            KeyState::Rotating => "rotating",
        },
        remote_id: key.remote_id.clone(),
        registered_at: key.registered_at.clone(),
    }
}

#[tauri::command]
pub fn list_keys() -> KeyList {
    let mut keys = Vec::new();
    let mut errors = Vec::new();

    for entry in Wiring::get().keyring().list() {
        match entry {
            Ok(key) => keys.push(row(&key)),
            Err(message) => errors.push(message),
        }
    }
    keys.sort_by(|a, b| (&a.repo, &a.purpose).cmp(&(&b.repo, &b.purpose)));

    KeyList { keys, errors }
}

/// 붙여넣은 값을 리포로 읽는다.
///
/// git 주소든 `owner/repo` 든 **로컬 리포 경로**든 받는다. 경로면 그 디렉토리의
/// origin 을 git 에게 묻는다 — 소유자와 이름을 손으로 옮겨 적으면 그때마다 틀린다.
#[tauri::command]
pub fn resolve_repo(text: String) -> Result<ResolvedRepo, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(String::new());
    }

    if let Some(repo) = RepoRef::parse(trimmed) {
        return Ok(shape(&repo));
    }

    let dir = expand(trimmed);
    if !dir.is_dir() {
        return Err("git 주소나 리포 디렉토리를 넣으세요".into());
    }

    let url = secrets_local::keys::origin_of(&dir).map_err(|e| e.to_string())?;
    RepoRef::parse(&url)
        .map(|repo| shape(&repo))
        .ok_or_else(|| format!("GitHub 리포가 아닙니다: {url}"))
}

fn shape(repo: &RepoRef) -> ResolvedRepo {
    ResolvedRepo {
        owner: repo.owner().to_string(),
        name: repo.name().to_string(),
        slug: repo.slug(),
    }
}

/// `~` 를 홈으로 편다. 사람이 손으로 치는 경로는 거의 이 형태다.
fn expand(text: &str) -> std::path::PathBuf {
    match text.strip_prefix("~/") {
        Some(rest) => std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
            .join(rest),
        None => std::path::PathBuf::from(text),
    }
}

/// 오래 걸리는 일을 하나의 job 으로 감싸 터미널 패널에 흘린다.
fn run<T>(
    app: &AppHandle,
    label: String,
    work: impl FnOnce(&JobPanel) -> Result<T, String>,
) -> Result<T, String> {
    let job = next_job_id();
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
    let result = work(&panel);

    let _ = app.emit(
        "cli:end",
        Ended {
            job,
            ok: result.is_ok(),
            message: match &result {
                Ok(_) => label,
                Err(e) => e.clone(),
            },
        },
    );
    let _ = app.emit("keys:updated", ());
    result
}

#[tauri::command]
pub fn create_deploy_key(
    app: AppHandle,
    account: String,
    repo: String,
    purpose: String,
    write: bool,
) -> Result<KeyRow, String> {
    let at = locate(&repo, &purpose)?;
    run(&app, format!("{} 배포 키 만들기", at.slug()), |panel| {
        Wiring::get()
            .keyring()
            .create(&account, &at, write, panel)
            .map(|key| row(&key))
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn retry_registration(app: AppHandle, repo: String, purpose: String) -> Result<KeyRow, String> {
    let at = locate(&repo, &purpose)?;
    run(&app, format!("{} 등록 다시 시도", at.slug()), |panel| {
        Wiring::get()
            .keyring()
            .retry(&at, panel)
            .map(|key| row(&key))
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn rotate_key(app: AppHandle, repo: String, purpose: String) -> Result<KeyRow, String> {
    let at = locate(&repo, &purpose)?;
    run(&app, format!("{} 재발급", at.slug()), |panel| {
        Wiring::get()
            .keyring()
            .rotate(&at, panel)
            .map(|key| row(&key))
            .map_err(|e| e.to_string())
    })
}

/// 이 키가 무엇에 쓰이는지를 바꾼다. 로컬에서만 일어나므로 job 으로 감싸지 않는다.
#[tauri::command]
pub fn set_purpose(repo: String, purpose: String, to: String) -> Result<KeyRow, String> {
    let at = locate(&repo, &purpose)?;
    Wiring::get()
        .keyring()
        .set_purpose(&at, to.trim())
        .map(|key| row(&key))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_key(app: AppHandle, repo: String, purpose: String) -> Result<(), String> {
    let at = locate(&repo, &purpose)?;
    run(&app, format!("{} 삭제", at.slug()), |panel| {
        Wiring::get()
            .keyring()
            .remove(&at, panel)
            .map_err(|e| e.to_string())
    })
}

/// GitHub 에는 있는데 이 금고에 개인 키가 없는 것.
///
/// 원격을 실제로 묻는 유일한 명령이다. 목록은 로컬만 읽어 빠르게 뜨고, 이건
/// 사용자가 누를 때만 돈다.
#[tauri::command]
pub fn scan_unowned(app: AppHandle, account: String) -> Result<Vec<UnownedRow>, String> {
    run(&app, format!("{account} GitHub 키 조회"), |panel| {
        Wiring::get()
            .keyring()
            .unowned(&account, panel)
            .map(|found| {
                found
                    .into_iter()
                    .map(|key| UnownedRow {
                        r#ref: format!("unowned/{}", key.id),
                        domain: "github",
                        account: account.clone(),
                        title: key.title,
                        repo: key.repo,
                        fingerprint: key.fingerprint,
                        remote_id: key.id,
                        registered_at: key.created_at,
                    })
                    .collect()
            })
            .map_err(|e| e.to_string())
    })
}

/// 개인 키를 클립보드로 넘긴다. 금고 밖으로 나가는 유일한 자리다.
#[tauri::command]
pub fn reveal_private_key(repo: String, purpose: String) -> Result<String, String> {
    let at = locate(&repo, &purpose)?;
    Wiring::get()
        .keyring()
        .private_key(&at)
        .map(|secret| secret.expose().to_string())
        .map_err(|e| e.to_string())
}

/// `~/.ssh/config` 에 적힌 호스트. 읽기만 한다.
#[tauri::command]
pub fn ssh_hosts() -> Vec<HostRow> {
    secrets_local::keys::hosts::known()
        .into_iter()
        .map(|host| HostRow {
            alias: host.alias,
            address: host.address,
            user: host.user,
        })
        .collect()
}
