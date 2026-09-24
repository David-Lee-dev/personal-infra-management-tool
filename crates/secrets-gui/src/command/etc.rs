//! 기타 — 다시 받을 수 없는 파일. 기록 읽기 · 용도 · 소비처 기록 · 여는 값 꺼내기.
//!
//! 들인 파일과 여는 값은 바꾸지 않는다. 소비처의 파일도 건드리지 않는다.

use secrets_core::etc::{EtcItem, EtcRef};
use tauri::{AppHandle, Emitter};

use crate::command::keys::tilde;
use crate::dto::*;
use crate::wiring::Wiring;

fn at(place: &EtcWhere) -> EtcRef {
    EtcRef {
        project: place.project.clone(),
        name: place.name.clone(),
    }
}

/// 기록의 시각(`2026-09-23T18:15:26+09:00`)을 화면용 `2026-09-23 18:15` 로.
fn minute(stamp: &str) -> String {
    stamp.get(..16).unwrap_or(stamp).replace('T', " ")
}

fn row(item: &EtcItem) -> EtcRow {
    let dir = secrets_local::etc::dir_of(&item.at());
    EtcRow {
        r#ref: format!("etc/{}", item.at().slug()),
        project: item.project.clone(),
        name: item.name.clone(),
        kind: item.kind.clone(),
        purpose: item.purpose.clone(),
        path: tilde(&dir),
        absolute: dir.display().to_string(),
        file: EtcFileRow {
            name: item.file.clone(),
            size: item.size,
            sha256: item.sha256.chars().take(12).collect(),
            adopted_at: minute(&item.adopted_at),
        },
        values: item.values.clone(),
        consumers: item
            .consumers
            .iter()
            .map(|c| EtcConsumerRow {
                host: c.host.clone(),
                file: c.file.clone(),
                recorded_at: minute(&c.recorded_at),
            })
            .collect(),
    }
}

#[tauri::command]
pub fn list_etc() -> EtcList {
    use secrets_core::etc::EtcVault;

    let mut items = Vec::new();
    let mut errors = Vec::new();
    for entry in Wiring::get().etc_vault().list() {
        match entry {
            Ok(item) => items.push(row(&item)),
            Err(message) => errors.push(message),
        }
    }
    EtcList { items, errors }
}

#[tauri::command]
pub fn set_etc_purpose(app: AppHandle, at: EtcWhere, to: String) -> Result<EtcRow, String> {
    let item = Wiring::get()
        .etc_book()
        .set_purpose(&self::at(&at), &to)
        .map_err(|e| e.to_string())?;
    let _ = app.emit("keys:updated", ());
    Ok(row(&item))
}

/// 사본이 놓인 곳을 기록한다. 그 파일은 건드리지 않는다.
#[tauri::command]
pub fn add_etc_consumer(app: AppHandle, at: EtcWhere, place: EtcPlace) -> Result<EtcRow, String> {
    let item = Wiring::get()
        .etc_book()
        .add_consumer(&self::at(&at), &place.host, &place.file)
        .map_err(|e| e.to_string())?;
    let _ = app.emit("keys:updated", ());
    Ok(row(&item))
}

/// 기록에서 뺀다. 그 파일은 건드리지 않는다.
#[tauri::command]
pub fn remove_etc_consumer(app: AppHandle, at: EtcWhere, place: EtcPlace) -> Result<EtcRow, String> {
    let item = Wiring::get()
        .etc_book()
        .remove_consumer(&self::at(&at), &place.host, &place.file)
        .map_err(|e| e.to_string())?;
    let _ = app.emit("keys:updated", ());
    Ok(row(&item))
}

/// 여는 값 하나. 복사하려고 꺼낸다 — 값이 금고 밖으로 나가는 자리다.
#[tauri::command]
pub fn etc_value(at: EtcWhere, name: String) -> Result<String, String> {
    Wiring::get()
        .etc_book()
        .value(&self::at(&at), &name)
        .map(|secret| secret.expose().to_string())
        .map_err(|e| e.to_string())
}

/// Android `key.properties` 에 그대로 붙일 줄들. 값들과 금고의 키스토어 절대 경로.
#[tauri::command]
pub fn etc_key_properties(at: EtcWhere) -> Result<String, String> {
    use secrets_core::etc::EtcVault;

    let wiring = Wiring::get();
    let item = wiring.etc_vault().load(&self::at(&at)).map_err(|e| e.to_string())?;
    if item.kind != "android" {
        return Err(format!("{} 는 Android 업로드 키가 아닙니다", item.at().slug()));
    }
    let book = wiring.etc_book();
    let mut lines = String::new();
    for name in &item.values {
        let value = book.value(&item.at(), name).map_err(|e| e.to_string())?;
        lines.push_str(&format!("{name}={}\n", value.expose()));
    }
    lines.push_str(&format!(
        "storeFile={}\n",
        secrets_local::etc::file_of(&item).display()
    ));
    Ok(lines)
}
