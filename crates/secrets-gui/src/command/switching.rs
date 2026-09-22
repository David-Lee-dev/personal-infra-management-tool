//! 전역 전환과 아카이브.

use secrets_core::account;
use secrets_local::vault::store;
use secrets_local::switching;
use tauri::{AppHandle, Emitter};

use crate::progress::*;

/// 이 계정을 전역으로 활성화한다.
///
/// 자리에 있던 실물은 지우지 않고 보관소로 옮긴다.
#[tauri::command]
pub fn activate_account(app: AppHandle, provider: String, slug: String) -> Result<(), String> {
    let provider = account::Provider::parse(&provider)
        .ok_or_else(|| format!("알 수 없는 provider: {provider}"))?;
    let acc = store::load(provider, &slug).map_err(|e| format!("계정을 읽지 못했습니다: {e}"))?;

    let job = next_job_id();
    let label = format!("{}/{} 전역 전환", provider.id(), slug);
    let _ = app.emit(
        "cli:start",
        Started {
            job: job.clone(),
            command: label.clone(),
        },
    );

    let emit_line = |line: String| {
        let _ = app.emit(
            "cli:line",
            Line {
                job: job.clone(),
                stream: "out",
                line,
            },
        );
    };

    let result = switching::activate(&acc);
    let (ok, message) = match &result {
        Ok(switched) => {
            emit_line(format!("{} → 이 계정", switched.linked.display()));
            if let Some(archived) = &switched.archived {
                emit_line(format!(
                    "자리에 있던 설정을 보관했습니다: {}",
                    archived.display()
                ));
            }
            if let Some(email) = &switched.git_email {
                emit_line(format!("커밋 이메일: {email}"));
            }
            (true, format!("{label} — 완료"))
        }
        Err(e) => (false, format!("{label} — 실패: {e}")),
    };

    let _ = app.emit("cli:end", Ended { job, ok, message });
    let _ = app.emit("accounts:updated", ());
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// 계정을 아카이브로 내린다.
///
/// 지우지 않고 옮긴다. 자격이 이미 죽었더라도 무엇을 언제 썼는지는 남아야 한다.
#[tauri::command]
pub fn archive_account(app: AppHandle, provider: String, slug: String) -> Result<(), String> {
    let provider = account::Provider::parse(&provider)
        .ok_or_else(|| format!("알 수 없는 provider: {provider}"))?;
    let acc = store::load(provider, &slug).map_err(|e| format!("계정을 읽지 못했습니다: {e}"))?;

    let job = next_job_id();
    let label = format!("{}/{slug} 삭제", provider.id());
    let _ = app.emit(
        "cli:start",
        Started {
            job: job.clone(),
            command: label.clone(),
        },
    );

    let emit_line = |line: String| {
        let _ = app.emit(
            "cli:line",
            Line {
                job: job.clone(),
                stream: "out",
                line,
            },
        );
    };

    // 전역으로 쓰이는 계정을 그냥 옮기면 링크가 끊어져 CLI 가 통째로 망가진다.
    // 먼저 걷어내고 보관된 설정으로 돌아갈 수 있게 한다.
    if switching::is_active(&acc) {
        if let Err(e) = switching::deactivate(provider) {
            let message = format!("{label} — 전역 링크를 걷어내지 못했습니다: {e}");
            let _ = app.emit(
                "cli:end",
                Ended {
                    job,
                    ok: false,
                    message: message.clone(),
                },
            );
            return Err(message);
        }
        emit_line("전역 링크를 걷어냈습니다".into());
    }

    let result = store::archive_account(provider, &slug, account::ArchiveReason::Deleted);
    let (ok, message) = match &result {
        Ok(moved) => {
            // 어디에 남았는지는 알려 주되, 한 일은 삭제다.
            emit_line(format!("보관 위치: {}", moved.display()));
            (true, format!("{label} — 삭제했습니다"))
        }
        Err(e) => (false, format!("{label} — 실패: {e}")),
    };

    let _ = app.emit("cli:end", Ended { job, ok, message });
    let _ = app.emit("accounts:updated", ());
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// 전역 링크를 걷어낸다. 계정은 그대로 둔다.
#[tauri::command]
pub fn deactivate_provider(app: AppHandle, provider: String) -> Result<(), String> {
    let provider = account::Provider::parse(&provider)
        .ok_or_else(|| format!("알 수 없는 provider: {provider}"))?;
    switching::deactivate(provider).map_err(|e| e.to_string())?;
    let _ = app.emit("accounts:updated", ());
    Ok(())
}
