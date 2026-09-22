//! 계정 목록·등록·교체·검증.

use secrets_core::port;
use secrets_core::{account, enrollment};
use secrets_local::switching;
use secrets_local::vault::{paths, store};
use tauri::{AppHandle, Emitter};

use crate::dto::*;
use crate::progress::*;
use crate::wiring::Wiring;

#[tauri::command]
pub fn list_accounts() -> AccountList {
    let mut accounts = Vec::new();
    let mut errors = Vec::new();
    // 만료 판정의 기준이 되는 오늘. 목록 한 번에 한 번만 읽는다.
    let today = secrets_core::port::Clock::today(&Wiring::get().clock);

    for entry in store::list() {
        match entry {
            Ok(acc) => accounts.push(AccountRow {
                slug: acc.slug.clone(),
                provider: acc.provider.id(),
                display: acc.display.clone(),
                note: acc.note.clone(),
                identity_kind: acc.identity.kind.clone(),
                identity_name: acc.identity.name.clone(),
                cli_home: paths::cli_home(&acc).display().to_string(),
                verified_at: acc.verification.as_ref().map(|v| v.checked_at.clone()),
                verified_ok: acc.verification.as_ref().map(|v| v.ok),
                verified_detail: acc.verification.as_ref().map(|v| v.detail.clone()),
                expires: acc.expires.clone(),
                expiry: match acc.expiry_on(&today) {
                    account::Expiry::Unset => "unset",
                    account::Expiry::Never => "never",
                    account::Expiry::Ok => "ok",
                    account::Expiry::Soon(_) => "soon",
                    account::Expiry::Expired(_) => "expired",
                },
                expiry_days: match acc.expiry_on(&today) {
                    account::Expiry::Soon(d) | account::Expiry::Expired(d) => Some(d),
                    _ => None,
                },
                renewal_hint: acc.renewal_hint(),
                scopes: acc.scopes.clone(),
                replacements: store::history(&acc).len(),
                is_active: switching::is_active(&acc),
                global_path: switching::link_for(&acc).map(|l| l.global.display().to_string()),
                caution: switching::caution(acc.provider),
                git_email: acc.git_email.clone(),
                aws_account_id: acc.aws_account_id.clone(),
                root_keys_present: acc.root_keys_present,
                root_mfa: acc.root_mfa,
            }),
            Err(message) => errors.push(message),
        }
    }

    AccountList {
        alerts: accounts
            .iter()
            .filter(|a| a.expiry == "soon" || a.expiry == "expired")
            .map(|a| match (a.expiry, a.expiry_days) {
                ("expired", Some(d)) => {
                    format!("{}/{} 자격이 {d}일 전에 만료됐습니다", a.provider, a.slug)
                }
                ("soon", Some(0)) => format!("{}/{} 자격이 오늘 만료됩니다", a.provider, a.slug),
                ("soon", Some(d)) => {
                    format!("{}/{} 자격이 {d}일 뒤 만료됩니다", a.provider, a.slug)
                }
                _ => format!("{}/{} 만료 확인 필요", a.provider, a.slug),
            })
            .collect(),
        accounts,
        errors,
    }
}

/// 화면이 보내오는 것. 확인 단계가 읽어 온 사실은 여기 없다.
///
/// 신원·권한·만료일을 화면에서 받아 적으면 화면이 그 값을 고쳐 보낼 수 있다.
/// 그런 값은 준비 표가 가리키는 관찰 결과에서만 온다.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAccount {
    /// 자격 확인이 돌려준 표.
    pub preparation: String,
    pub slug: String,
    #[serde(default)]
    pub display: String,
    #[serde(default)]
    pub note: String,
}

/// 확인된 자격을 계정으로 확정한다.
#[tauri::command]
pub fn create_account(app: AppHandle, account: NewAccount) -> Result<(), String> {
    let NewAccount {
        preparation,
        slug,
        display,
        note,
    } = account;

    let id = port::PreparationId::named(&preparation);
    let draft = enrollment::Draft {
        slug,
        display,
        note,
    };

    let job = next_job_id();
    let label = format!("{} 등록", draft.slug);
    let _ = app.emit(
        "cli:start",
        Started {
            job: job.clone(),
            command: label.clone(),
        },
    );

    let made = Wiring::get().enrollment().register(&id, draft);
    let message = match &made {
        Ok(acc) => format!("{label} — {} 로 확인됨", acc.identity.name),
        Err(e) => format!("{label} — 실패: {e}"),
    };
    let _ = app.emit(
        "cli:end",
        Ended {
            job,
            ok: made.is_ok(),
            message,
        },
    );
    let _ = app.emit("accounts:updated", ());

    made.map(|_| ()).map_err(|e| e.to_string())
}

/// 확인된 새 자격으로 이 계정의 자격을 교체한다.
///
/// 확인·교체·기록·저장 중 어디서 실패하든 계정은 손대기 전 상태로 남는다.
/// 실패한 교체는 이력에도 남지 않는다.
#[tauri::command]
pub fn replace_credential(
    app: AppHandle,
    provider: String,
    slug: String,
    preparation: String,
) -> Result<(), String> {
    let provider = account::Provider::parse(&provider)
        .ok_or_else(|| format!("알 수 없는 provider: {provider}"))?;
    let acc = store::load(provider, &slug).map_err(|e| format!("계정을 읽지 못했습니다: {e}"))?;

    let job = next_job_id();
    let label = format!("{}/{} 자격 교체", provider.id(), slug);
    let _ = app.emit(
        "cli:start",
        Started {
            job: job.clone(),
            command: label.clone(),
        },
    );

    let id = port::PreparationId::named(&preparation);
    let done = Wiring::get().enrollment().reissue(&acc, &id);

    let message = match &done {
        Ok(updated) => {
            let until = match updated.expires.as_deref() {
                Some(account::NEVER) | None => "기한 없음".to_string(),
                Some(date) => format!("{date} 까지"),
            };
            format!("{label} — {} · {until}", updated.identity.name)
        }
        Err(e) => format!("{label} — 실패: {e}"),
    };
    let _ = app.emit(
        "cli:end",
        Ended {
            job,
            ok: done.is_ok(),
            message,
        },
    );
    let _ = app.emit("accounts:updated", ());

    done.map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn verify_account(app: AppHandle, provider: String, slug: String) -> Result<(), String> {
    let provider = account::Provider::parse(&provider)
        .ok_or_else(|| format!("알 수 없는 provider: {provider}"))?;
    let acc = store::load(provider, &slug).map_err(|e| format!("계정을 읽지 못했습니다: {e}"))?;

    std::thread::spawn(move || {
        let job = next_job_id();
        let label = format!("{}/{} 검증", acc.provider.id(), acc.slug);
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
        let (ok, message) = match Wiring::get().enrollment().recheck(&acc, &panel) {
            Ok(checked) => match checked.verification.as_ref() {
                Some(v) if v.ok => (true, format!("{label} — {} 로 확인됨", checked.identity.name)),
                Some(v) => (false, format!("{label} — {}", v.detail)),
                None => (false, format!("{label} — 확인 결과가 없습니다")),
            },
            Err(e) => (false, format!("{label} — 실패: {e}")),
        };

        let _ = app.emit("cli:end", Ended { job, ok, message });
        let _ = app.emit("accounts:updated", ());
    });

    Ok(())
}
