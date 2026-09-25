//! SSH 접속 — 별칭마다 어느 인스턴스의 어느 계정인지 정하고, 그룹별 conf 파일을 만든다.
//!
//! `~/.ssh/config` 에는 `Include` 한 줄만 더한다(버튼으로, 원본을 보관한 뒤).

use secrets_core::ssh::{Overview, SshHost, alias_for};
use tauri::{AppHandle, Emitter};

use crate::command::keys::tilde;
use crate::dto::*;
use crate::wiring::Wiring;

const UPDATED: &str = "ssh:updated";

fn overview_row(overview: Overview) -> SshOverviewRow {
    use secrets_core::project::ProjectStore;

    let wiring = Wiring::get();
    let files = wiring.ssh_files();
    let mut groups: Vec<SshGroupRow> = Vec::new();
    for view in overview.hosts {
        let host = SshHostRow {
            alias: view.host.alias.clone(),
            instance: view.host.instance.clone(),
            login: view.host.login.clone(),
            instance_name: view
                .seat
                .as_ref()
                .map(|s| s.instance_name.clone())
                .unwrap_or_default(),
            address: view.seat.as_ref().map(|s| s.address.clone()),
            found: view.seat.is_some(),
        };
        match groups.iter_mut().find(|g| g.group == view.host.group) {
            Some(group) => group.hosts.push(host),
            None => groups.push(SshGroupRow {
                file: tilde(&files.dir.join(format!("{}.conf", view.host.group))),
                group: view.host.group,
                hosts: vec![host],
            }),
        }
    }

    // 그룹 이름 후보 — 프로젝트 그룹과 이미 쓰는 SSH 그룹.
    let mut known_groups: Vec<String> = wiring
        .project_store()
        .list()
        .into_iter()
        .filter_map(Result::ok)
        .map(|r| r.group)
        .chain(groups.iter().map(|g| g.group.clone()))
        .collect();
    known_groups.sort();
    known_groups.dedup();

    let instances = wiring
        .server_link()
        .instances()
        .into_iter()
        .map(|i| SshInstanceRow {
            accounts: i
                .accounts
                .iter()
                .map(|a| SshAccountRow {
                    alias: alias_for(&i.name, &i.instance, &a.login),
                    login: a.login.clone(),
                })
                .collect(),
            instance: i.instance,
            name: i.name,
            address: i.address,
        })
        .collect();

    SshOverviewRow {
        include_line: files.include_line(),
        includes_ours: overview.user.includes_ours,
        user_config: tilde(&files.user_config),
        duplicates: overview.duplicates,
        groups,
        known_groups,
        instances,
    }
}

/// 지금 상태. 보기 전에 conf 파일을 다시 만들어 시크릿 저장소의 최신 계정을 반영한다.
#[tauri::command]
pub async fn ssh_overview() -> Result<SshOverviewRow, String> {
    let config = Wiring::get().ssh_config();
    config.regenerate().map_err(|e| e.to_string())?;
    config
        .overview()
        .map(overview_row)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_ssh_host(app: AppHandle, form: SshHostForm) -> Result<SshOverviewRow, String> {
    let host = SshHost {
        alias: form.alias,
        group: form.group,
        instance: form.instance,
        login: form.login,
    };
    let overview = Wiring::get()
        .ssh_config()
        .add(&host)
        .map_err(|e| e.to_string())?;
    let _ = app.emit(UPDATED, ());
    Ok(overview_row(overview))
}

/// 별칭을 뺀다. 서버 계정과 키는 그대로다.
#[tauri::command]
pub async fn remove_ssh_host(app: AppHandle, alias: String) -> Result<SshOverviewRow, String> {
    let overview = Wiring::get()
        .ssh_config()
        .remove(&alias)
        .map_err(|e| e.to_string())?;
    let _ = app.emit(UPDATED, ());
    Ok(overview_row(overview))
}

/// `~/.ssh/config` 맨 위에 `Include` 한 줄을 넣는다. 보관한 원본의 자리를 돌려준다.
#[tauri::command]
pub async fn add_ssh_include(app: AppHandle) -> Result<Option<String>, String> {
    let kept = Wiring::get()
        .ssh_config()
        .add_include()
        .map_err(|e| e.to_string())?;
    let _ = app.emit(UPDATED, ());
    Ok(kept.map(|p| tilde(std::path::Path::new(&p))))
}
