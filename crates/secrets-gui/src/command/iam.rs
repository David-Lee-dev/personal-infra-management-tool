//! IAM — 만들기 · 소비처 기록 · 삭제.
//!
//! AWS 에 닿는 일은 전부 job 으로 감싸 터미널 칸에 흘린다. 목록 · 미리 보기 ·
//! 소비처 기록은 로컬만 다룬다. 소비처의 파일은 건드리지 않는다.

use secrets_core::aws::iam::{Draft, Env, IamRef, IamUser, Policy, policy::Effect};
use tauri::{AppHandle, Emitter};

use crate::command::keys::tilde;
use crate::dto::*;
use crate::progress::*;
use crate::wiring::Wiring;

fn at(place: &IamWhere) -> IamRef {
    IamRef {
        account: place.account.clone(),
        name: place.name.clone(),
    }
}

fn rules(policy: &Policy) -> Vec<IamRuleRow> {
    policy
        .statements
        .iter()
        .map(|st| IamRuleRow {
            effect: if st.effect == Effect::Allow { "허용" } else { "거부" },
            actions: st.actions.join(" · "),
            target: st.resources.join(" · "),
            condition: st.conditions.join(" · "),
        })
        .collect()
}

fn service_label(service: &str) -> String {
    match service {
        "s3" => "S3".into(),
        "bedrock" => "Bedrock".into(),
        "ses" => "SES".into(),
        "sqs" => "SQS".into(),
        "sns" => "SNS".into(),
        other => other.to_string(),
    }
}

/// ARN 에서 자원 부분만. `arn:aws:s3:::bucket/x` → `bucket/x`.
fn short(resource: &str) -> &str {
    resource.splitn(6, ':').nth(5).unwrap_or(resource)
}

fn scope(policy: &Policy) -> String {
    let mut targets: Vec<&str> = policy.resources().into_iter().map(short).collect();
    targets.dedup();
    match targets.as_slice() {
        [one] => one.to_string(),
        many => format!("대상 {}", many.len()),
    }
}

fn row(user: &IamUser) -> IamRow {
    let issuer = Wiring::get().issuer();
    let policy = issuer.policy(&user.at()).ok();
    IamRow {
        r#ref: user.at().slug(),
        account: user.account.clone(),
        name: user.name.clone(),
        app: user.app.clone(),
        env: user.env.clone(),
        perm: user.perm.clone(),
        purpose: user.purpose.clone(),
        master: user.master.clone(),
        key_id: user.key_id.clone(),
        issued_at: user.issued_at.clone(),
        created_at: user.created_at.clone(),
        service: policy
            .as_ref()
            .map(|p| p.services().iter().map(|s| service_label(s)).collect::<Vec<_>>().join(" · "))
            .unwrap_or_default(),
        scope: policy.as_ref().map(scope).unwrap_or_default(),
        rules: policy.as_ref().map(rules).unwrap_or_default(),
        consumers: user
            .consumers
            .iter()
            .map(|c| IamConsumerRow {
                host: c.host.clone(),
                file: c.file.clone(),
                id_variable: c.id_variable.clone(),
                secret_variable: c.secret_variable.clone(),
                recorded_at: c.recorded_at.clone(),
            })
            .collect(),
        path: tilde(&secrets_local::iam::vault::dir_of(&user.at())),
        deletable_from: user.deletable_from.clone(),
        checked_at: user.checked.as_ref().map(|c| c.checked_at.clone()),
        last_use: user
            .checked
            .as_ref()
            .and_then(|c| c.last.as_ref())
            .map(|used| IamLastUse {
                at: used.at.clone(),
                service: used.service.clone(),
                region: used.region.clone(),
            }),
    }
}

#[tauri::command]
pub fn list_iam() -> IamList {
    let mut users = Vec::new();
    let mut errors = Vec::new();
    for entry in Wiring::get().issuer().list() {
        match entry {
            Ok(user) => users.push(row(&user)),
            Err(message) => errors.push(message),
        }
    }
    users.sort_by(|a, b| a.name.cmp(&b.name));
    IamList { users, errors }
}

fn env_of(text: &str) -> Result<Env, String> {
    Env::parse(text).ok_or_else(|| format!("환경은 prod · dev · local 중 하나입니다: {text}"))
}

/// 치는 동안 정책을 읽고 이름을 정한다. 로컬만 본다.
#[tauri::command]
pub fn preview_iam(draft: IamDraft) -> IamPreview {
    let empty = |error: String| IamPreview {
        name: None,
        rules: Vec::new(),
        problems: Vec::new(),
        error: Some(error),
    };
    if draft.policy.trim().is_empty() {
        return empty(String::new());
    }
    let policy = match Policy::read(&draft.policy) {
        Ok(policy) => policy,
        Err(e) => return empty(e.0),
    };
    let env = match env_of(&draft.env) {
        Ok(env) => env,
        Err(e) => return empty(e),
    };

    let named = if draft.app.trim().is_empty() {
        Err(String::new())
    } else {
        Wiring::get()
            .issuer()
            .name_for(&draft.account, &draft.app, env, Some(&draft.perm), &policy)
            .map(|name| name.full())
            .map_err(|e| e.to_string())
    };
    IamPreview {
        rules: rules(&policy),
        problems: policy.problems(),
        name: named.as_ref().ok().cloned(),
        error: named.err(),
    }
}

/// 오래 걸리는 일을 하나의 job 으로 감싸 터미널 패널에 흘린다. 성공했을 때만 목록을 다시 읽게 한다.
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
    if result.is_ok() {
        let _ = app.emit("keys:updated", ());
    }
    result
}

#[tauri::command]
pub fn create_iam(app: AppHandle, draft: IamDraft) -> Result<IamRow, String> {
    let env = env_of(&draft.env)?;
    let perm = draft.perm.trim();
    let draft = Draft {
        master: draft.master,
        account: draft.account,
        app: draft.app,
        env,
        perm: (!perm.is_empty()).then(|| perm.to_string()),
        purpose: draft.purpose,
        policy: draft.policy,
    };
    run(&app, format!("IAM {} 만들기", draft.app.trim()), |panel| {
        Wiring::get()
            .issuer()
            .create(&draft, panel)
            .map(|user| row(&user))
            .map_err(|e| e.to_string())
    })
}

/// 이 키를 어디에 넣었는지 적는다. 파일은 건드리지 않는다.
#[tauri::command]
pub fn add_iam_consumer(app: AppHandle, at: IamWhere, place: IamPlace) -> Result<IamRow, String> {
    let user = Wiring::get()
        .issuer()
        .add_consumer(&self::at(&at), &place.host, &place.file, &place.id_variable)
        .map_err(|e| e.to_string())?;
    let _ = app.emit("keys:updated", ());
    Ok(row(&user))
}

/// 기록에서 뺀다. 파일은 건드리지 않는다.
#[tauri::command]
pub fn remove_iam_consumer(app: AppHandle, at: IamWhere, place: IamPlace) -> Result<IamRow, String> {
    let user = Wiring::get()
        .issuer()
        .remove_consumer(&self::at(&at), &place.host, &place.file, &place.id_variable)
        .map_err(|e| e.to_string())?;
    let _ = app.emit("keys:updated", ());
    Ok(row(&user))
}

/// `.env` 에 그대로 붙일 두 줄. 시크릿이 금고 밖으로 나가는 유일한 자리다.
///
/// 변수 이름을 주면 그 이름으로, 비우면 권한에서 정한 이름으로 적는다.
#[tauri::command]
pub fn iam_env_lines(at: IamWhere, id_variable: String) -> Result<String, String> {
    let issuer = Wiring::get().issuer();
    let user = issuer.load(&self::at(&at)).map_err(|e| e.to_string())?;
    let id_variable = if id_variable.trim().is_empty() {
        let perm: String = user
            .perm
            .to_ascii_uppercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        format!("AWS_{perm}_ACCESS_KEY_ID")
    } else {
        id_variable.trim().to_string()
    };
    let secret_variable = secrets_core::aws::iam::Consumer::secret_variable_for(&id_variable)
        .ok_or_else(|| format!("변수 이름이 규칙에 맞지 않습니다: {id_variable}"))?;
    let secret = issuer.secret(&self::at(&at)).map_err(|e| e.to_string())?;
    Ok(format!(
        "{id_variable}={}\n{secret_variable}={}\n",
        user.key_id,
        secret.expose()
    ))
}

/// 막혀도 목록을 다시 읽게 한다. 막히는 순간 AWS 에서 본 것으로 삭제 가능일이 바뀐다.
#[tauri::command]
pub fn remove_iam(app: AppHandle, at: IamWhere) -> Result<(), String> {
    let done = run(&app, format!("{} 삭제", at.name), |panel| {
        Wiring::get()
            .issuer()
            .remove(&self::at(&at), panel)
            .map_err(|e| e.to_string())
    });
    if done.is_err() {
        let _ = app.emit("keys:updated", ());
    }
    done
}

/// 로컬에서만 일어난다. job 으로 감싸지 않는다.
#[tauri::command]
pub fn set_iam_purpose(app: AppHandle, at: IamWhere, to: String) -> Result<IamRow, String> {
    let user = Wiring::get()
        .issuer()
        .set_purpose(&self::at(&at), &to)
        .map_err(|e| e.to_string())?;
    let _ = app.emit("keys:updated", ());
    Ok(row(&user))
}

/// AWS 에 마지막 사용을 묻고 기록한다. 삭제 가능일도 다시 정해진다.
#[tauri::command]
pub fn iam_last_used(app: AppHandle, at: IamWhere) -> Result<IamRow, String> {
    run(&app, format!("{} 마지막 사용", at.name), |panel| {
        Wiring::get()
            .issuer()
            .last_used(&self::at(&at), panel)
            .map(|user| row(&user))
            .map_err(|e| e.to_string())
    })
}
