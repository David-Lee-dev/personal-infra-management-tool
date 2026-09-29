//! AWS pem 키를 금고로 들이는 일.
//!
//! 사용자가 키페어와 개인 키 자리를 직접 말하고, 우리는 그것이 맞는지 AWS 에
//! 지목해 물어 확인한다. AWS 가 가진 것을 훑어 보여 주지 않는다 — 이 화면은
//! 들인 것만 말한다.

use secrets_core::aws::Machine;
use secrets_core::aws::pairing::{Pairer, Pairing};
use tauri::{AppHandle, Emitter};

use crate::command::keys::{expiry_row, tilde};
use crate::dto::*;
use crate::progress::*;
use crate::wiring::Wiring;

/// 이 금고가 쥐고 있는 pem 키.
///
/// 로컬 기록만 읽는다. AWS 에 묻지 않으므로 바로 뜬다.
#[tauri::command]
pub fn list_aws_keys() -> AwsKeyList {
    let mut keys = Vec::new();
    let mut errors = Vec::new();

    for entry in secrets_local::aws_vault::list() {
        match entry {
            Ok(record) => keys.push(held(&record)),
            Err(message) => errors.push(message),
        }
    }
    AwsKeyList { keys, errors }
}

fn held(record: &secrets_core::aws::KeyPairRecord) -> AwsHeldKeyRow {
    let at = secrets_local::aws_vault::dir_of(
        &record.account,
        &record.machine,
        &record.region,
        &record.name,
    );
    AwsHeldKeyRow {
        r#ref: record.slug(),
        name: record.name.clone(),
        account: record.account.clone(),
        machine: record.machine.clone(),
        region: record.region.clone(),
        fingerprint: record.fingerprint.clone(),
        verified: record.verified,
        purpose: record.purpose.clone(),
        adopted_at: record.adopted_at.clone(),
        path: tilde(&at),
        expiry: expiry_row(&record.expires),
    }
}

/// pem 키의 만료일을 적는다. 빈 값은 지운다. 기록만 바뀐다.
#[tauri::command]
pub fn set_pem_expires(app: AppHandle, at: PemWhere, to: String) -> Result<AwsHeldKeyRow, String> {
    let expires = secrets_core::expiry::check_expires(&to)?;
    let mut record = secrets_local::aws_vault::load(&at.account, &at.machine, &at.region, &at.name)
        .map_err(|e| e.to_string())?;
    record.expires = expires;
    secrets_local::aws_vault::save(&record).map_err(|e| e.to_string())?;
    let _ = app.emit("keys:updated", ());
    Ok(held(&record))
}

/// 손에 든 개인 키가 그 리전의 어느 키페어인지 본다.
///
/// 파일 이름을 키페어 이름으로 짚어 보되, 지문이 맞는 키페어가 있으면 그것이다.
/// 돌려주는 `name` 이 들일 때 쓸 AWS 의 이름이다.
///
/// 이 파일을 가리키는 `~/.ssh/config` 호스트도 같이 알려 준다. 금고로 옮기면 그
/// 접속이 끊기므로 미리 보여야 한다.
#[tauri::command]
pub fn check_private_key(
    app: AppHandle,
    account: Option<String>,
    machine: String,
    region: String,
    name: String,
    path: String,
) -> Result<PrivateKeyCheck, String> {
    // 어느 키페어의 pem 인지 AWS 에 물어 가린다. 물을 계정이 없으면 시작할 수 없다.
    let Some(account) = account else {
        return Err("pem 키를 확인할 AWS 계정이 없습니다. 이 pem이 어느 키페어의 것인지 AWS에 물어 확인하므로, \
                    계정 메뉴에서 AWS 계정을 먼저 추가하세요."
            .into());
    };
    let kind = match machine.as_str() {
        "lightsail" => Machine::Lightsail,
        _ => Machine::Ec2,
    };

    let pem = expand(&path);
    let mine = secrets_local::aws_vault::fingerprints_of(&pem).map_err(|e| e.to_string())?;

    let job = next_job_id();
    let _ = app.emit(
        "cli:start",
        Started {
            job: job.clone(),
            command: format!("{name} 확인"),
        },
    );
    let panel = JobPanel {
        app: app.clone(),
        job: job.clone(),
    };

    let gateway = Wiring::get().aws();
    let asked = gateway
        .account_id(&account, &panel)
        .and_then(|id| {
            let seen = gateway.key_pairs(&account, kind, &region, &panel)?;
            Ok((id, Pairer::pair(&name, &mine, &seen)?))
        })
        .map_err(|e| e.to_string());

    let _ = app.emit(
        "cli:end",
        Ended {
            job,
            ok: asked.is_ok(),
            message: match &asked {
                Ok(_) => format!("{name} 확인"),
                Err(e) => e.clone(),
            },
        },
    );

    let (account_id, paired) = asked?;
    Ok(PrivateKeyCheck {
        name: paired.name().to_string(),
        fingerprint: mine[0].clone(),
        // AWS 가 지문을 주지 않는 것이 있다. 확인한 척하지 않는다.
        verified: matches!(paired, Pairing::Verified(_)),
        account_id,
        referred_by: secrets_local::keys::hosts::referring_to(&pem),
    })
}

/// 개인 키를 금고로 들인다. 지문이 맞을 때만 한다.
#[tauri::command]
pub fn adopt_key_pair(app: AppHandle, adopt: Adoption) -> Result<AwsHeldKeyRow, String> {
    let record = secrets_core::aws::KeyPairRecord {
        name: adopt.name,
        account: adopt.account,
        machine: adopt.machine,
        region: adopt.region,
        fingerprint: String::new(),
        purpose: adopt.purpose,
        verified: false,
        adopted_at: String::new(),
        expires: None,
    };

    let kept = secrets_local::aws_vault::adopt(record, &expand(&adopt.path), adopt.expected.as_deref())
        .map_err(|e| e.to_string())?;
    let _ = app.emit("keys:updated", ());
    Ok(held(&kept))
}

/// `~` 를 홈으로 편다. 사람이 손으로 치는 경로는 거의 이 형태다.
pub(crate) fn expand(text: &str) -> std::path::PathBuf {
    match text.trim().strip_prefix("~/") {
        Some(rest) => {
            std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(rest)
        }
        None => std::path::PathBuf::from(text.trim()),
    }
}
