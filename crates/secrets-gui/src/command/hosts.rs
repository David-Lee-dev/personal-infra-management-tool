//! 인스턴스 접속 계정.
//!
//! pem 으로 서버에 들어가 계정을 만들고, 키를 심고, **그 키로 직접 들어가 확인**한다.
//! 무엇이 서버에서 돌았는지는 터미널 칸에 그대로 흐른다.

use secrets_core::aws::instance::{InstanceVault, Plan, Role, Seat};
use tauri::{AppHandle, Emitter};

use crate::dto::*;
use crate::progress::*;
use crate::wiring::Wiring;

fn locate(at: &Where) -> Result<Seat, String> {
    Seat::new(&at.region, &at.keypair, &at.instance, &at.account)
        .ok_or_else(|| format!("계정 이름으로 사용할 수 없습니다: {}", at.account))
}

fn shape(at: &Where, role: Role) -> Plan {
    Plan {
        role,
        workspace: at.workspace.clone(),
        group: at.group.clone(),
        via: at.via.clone(),
        address: at.address.clone(),
    }
}

fn role_of(text: &str) -> Role {
    match text {
        "admin" => Role::Admin,
        _ => Role::User,
    }
}

/// 그 pem 의 개인 키 자리. 이것으로 서버에 들어간다.
fn pem_of(at: &Where) -> String {
    secrets_local::aws_vault::dir_of(&at.aws_account, &at.machine, &at.region, &at.keypair)
        .join("key")
        .display()
        .to_string()
}

fn vault(at: &Where) -> secrets_local::hosts::FileAccounts {
    secrets_local::hosts::FileAccounts {
        aws_account: at.aws_account.clone(),
        machine: at.machine.clone(),
    }
}

fn readiness(found: &secrets_core::aws::instance::Readiness) -> HostReadiness {
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

/// 오래 걸리는 일을 하나의 job 으로 감싸 터미널 패널에 흘린다.
///
/// 목록이 바뀌었다는 알림은 **바꾼 명령만** 보낸다. 읽기만 하는 명령이 보내면
/// 화면이 다시 그려지면서 방금 보여 준 결과를 지운다.
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
    result
}

/// 바뀐 것이 있다고 알린다. 화면이 다시 읽는다.
fn changed(app: &AppHandle) {
    let _ = app.emit("keys:updated", ());
}

/// 이 금고가 들인 인스턴스 계정. 로컬 기록만 읽는다.
#[tauri::command]
pub fn list_instance_accounts(aws_account: String, machine: String) -> InstanceAccountList {
    let store = secrets_local::hosts::FileAccounts {
        aws_account,
        machine,
    };

    let mut accounts = Vec::new();
    let mut errors = Vec::new();
    for entry in store.list() {
        match entry {
            Ok(record) => accounts.push(InstanceAccountRow {
                r#ref: record.slug(),
                account: record.account.clone(),
                role: record.role.id(),
                purpose: record.purpose.clone(),
                instance: record.instance.clone(),
                instance_name: record.instance_name.clone(),
                address: record.address.clone(),
                keypair: record.keypair.clone(),
                region: record.region.clone(),
                via: record.via.clone(),
                fingerprint: record.fingerprint.clone(),
                workspace: record.workspace.clone(),
                group: record.group.clone(),
                state: match record.state {
                    secrets_core::aws::instance::AccountState::Local => "local",
                    secrets_core::aws::instance::AccountState::Installed => "installed",
                    secrets_core::aws::instance::AccountState::Verified => "verified",
                },
                verified_at: record.verified_at.clone(),
                ours: record.ours,
            }),
            Err(message) => errors.push(message),
        }
    }
    accounts.sort_by(|a, b| (&a.instance, &a.account).cmp(&(&b.instance, &b.account)));

    InstanceAccountList { accounts, errors }
}

/// 이 인스턴스가 계정을 받을 준비가 되었는지 본다. 아무것도 바꾸지 않는다.
#[tauri::command]
pub fn inspect_instance(app: AppHandle, at: Where) -> Result<HostReadiness, String> {
    let store = vault(&at);
    run(&app, format!("{} 점검", at.address), |panel| {
        Wiring::get()
            .provisioning(&store)
            .inspect(&pem_of(&at), &shape(&at, Role::User), panel)
            .map(|found| readiness(&found))
            .map_err(|e| e.to_string())
    })
}

/// 모자란 것을 채운다. 지금은 `acl` 하나다.
///
/// 패키지를 까는 건 이 도구가 서버 구성에 손대는 유일한 자리라, 사용자가 누를
/// 때만 돈다.
#[tauri::command]
pub fn prepare_instance(app: AppHandle, at: Where) -> Result<HostReadiness, String> {
    let store = vault(&at);
    run(&app, format!("{} 준비", at.address), |panel| {
        Wiring::get()
            .provisioning(&store)
            .prepare(&pem_of(&at), &shape(&at, Role::User), panel)
            .map(|found| readiness(&found))
            .map_err(|e| e.to_string())
    })
}

/// 계정을 만들고 키를 심고 그 키로 들어가 확인한다.
/// 한 인스턴스에 계정을 여럿 만든다. 앞에서부터 하나씩, 하나가 실패해도 나머지는 만든다.
///
/// 이름은 서버에 닿기 전에 전부 본다. 하나라도 쓸 수 없거나 겹치면 아무것도 만들지 않는다.
/// 목록 갱신은 끝에 한 번만 알린다 — 도중에 알리면 화면이 폼을 다시 그린다.
#[tauri::command]
pub fn create_instance_accounts(
    app: AppHandle,
    at: Where,
    accounts: Vec<NewSeat>,
    instance_name: String,
) -> Result<Vec<SeatOutcome>, String> {
    let seats = seats_of(&at, &accounts)?;
    let store = vault(&at);

    let mut outcomes = Vec::new();
    for (seat, wanted) in seats.iter().zip(&accounts) {
        let done = run(&app, format!("{} 계정 만들기", seat.slug()), |panel| {
            Wiring::get()
                .provisioning(&store)
                .create(
                    &pem_of(&at),
                    seat,
                    &shape(&at, role_of(&wanted.role)),
                    wanted.purpose.trim(),
                    &instance_name,
                    panel,
                )
                .map(|_| ())
                .map_err(|e| e.to_string())
        });
        outcomes.push(SeatOutcome {
            account: seat.account.clone(),
            error: done.err(),
        });
    }

    if outcomes.iter().any(|o| o.error.is_none()) {
        changed(&app);
    }
    Ok(outcomes)
}

fn seats_of(at: &Where, accounts: &[NewSeat]) -> Result<Vec<Seat>, String> {
    if accounts.is_empty() {
        return Err("생성할 계정이 없습니다.".into());
    }
    let mut seats: Vec<Seat> = Vec::new();
    for wanted in accounts {
        let seat = Seat::new(&at.region, &at.keypair, &at.instance, &wanted.account)
            .ok_or_else(|| format!("계정 이름으로 쓸 수 없습니다: {}", wanted.account))?;
        if seats.iter().any(|s| s.account == seat.account) {
            return Err(format!("{}이(가) 중복 입력되었습니다.", seat.account));
        }
        seats.push(seat);
    }
    Ok(seats)
}

/// Ghostty 새 창에서 그 계정으로 접속한다. 금고의 키를 쓴다.
#[tauri::command]
pub fn connect_instance_account(at: Where) -> Result<(), String> {
    let seat = locate(&at)?;
    let key = vault(&at).dir_of(&seat).join(secrets_local::hosts::vault::PRIVATE);
    secrets_local::hosts::terminal::open_ssh(&key, &seat.account, &at.address).map_err(|e| e.to_string())
}

/// 같은 키로 다시 심는다. 멈춘 자리에서도, 서버 설정이 바뀌었을 때도 쓴다.
#[tauri::command]
pub fn reinstall_instance_account(app: AppHandle, at: Where, role: String) -> Result<(), String> {
    let seat = locate(&at)?;
    let store = vault(&at);
    let done = run(&app, format!("{} SSH 키 다시 등록", seat.slug()), |panel| {
        Wiring::get()
            .provisioning(&store)
            .reinstall(&pem_of(&at), &seat, &shape(&at, role_of(&role)), panel)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    ;
    if done.is_ok() {
        changed(&app);
    }
    done
}

/// 권한·키·계정을 걷어낸다. 우리가 만들지 않은 계정은 남긴다.
#[tauri::command]
pub fn remove_instance_account(app: AppHandle, at: Where) -> Result<(), String> {
    let seat = locate(&at)?;
    let store = vault(&at);
    let done = run(&app, format!("{} 서버에서 제거", seat.slug()), |panel| {
        Wiring::get()
            .provisioning(&store)
            .remove(&pem_of(&at), &seat, panel)
            .map_err(|e| e.to_string())
    })
    ;
    if done.is_ok() {
        changed(&app);
    }
    done
}
