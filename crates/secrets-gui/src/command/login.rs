//! 자격 확인과 브라우저 로그인.

use std::collections::HashMap;

use secrets_core::port;
use secrets_core::credential::{self, secret};
use secrets_core::account;
use secrets_local::cli::tools;
use secrets_local::provider;
use tauri::{AppHandle, Emitter};

use crate::dto::*;
use crate::progress::*;
use crate::wiring::Wiring;


#[tauri::command]
pub fn provider_form(provider: String) -> Result<FormSpec, String> {
    let provider = account::Provider::parse(&provider)
        .ok_or_else(|| format!("알 수 없는 서비스: {provider}"))?;
    let method = provider::method(provider);

    Ok(FormSpec {
        fields: method
            .fields
            .iter()
            .map(|f| FieldSpec {
                key: f.key,
                label: f.label,
                secret: f.secret,
                help: f.help,
                required: f.required,
            })
            .collect(),
        guidance: method.guidance,
        browser_label: method.browser.map(|b| b.label),
        browser_url: method.browser.map(|b| b.url),
        tool_ready: tools::find_in_path(provider::tool_for(provider)).is_some(),
        tool: provider::tool_for(provider),
        flow: match method.flow {
            provider::LoginFlow::Credential => "credential",
            provider::LoginFlow::BrowserCallback => "browser",
            provider::LoginFlow::BrowserCode => "browser-code",
        },
    })
}

/// 화면이 채운 칸을 provider 에 맞는 자격으로 옮긴다.
pub fn credential_from(
    provider: account::Provider,
    mut values: HashMap<String, String>,
) -> credential::CredentialInput {
    let mut take = |key: &str| values.remove(key).unwrap_or_default();
    match provider {
        account::Provider::Github => credential::CredentialInput::Github {
            token: secret::Secret::new(take("token")),
        },
        account::Provider::Aws => credential::CredentialInput::Aws {
            access_key_id: take("access_key_id"),
            secret_access_key: secret::Secret::new(take("secret_access_key")),
        },
        account::Provider::Gcloud | account::Provider::Firebase => {
            credential::CredentialInput::Browser
        }
    }
}

/// 입력한 자격으로 신원을 미리 읽어 온다.
///
/// 계정을 만들기 전에 임시 홈에서 돌린다. 이름과 만료일을 사람이 추측해 적는
/// 대신 자격 자체에서 읽어 오기 위한 것이다.
#[tauri::command]
pub fn probe_credentials(
    app: AppHandle,
    provider: String,
    values: HashMap<String, String>,
) -> Result<ProbeResult, String> {
    let provider = account::Provider::parse(&provider)
        .ok_or_else(|| format!("알 수 없는 provider: {provider}"))?;

    let job = next_job_id();
    let label = format!("{} 자격 확인", provider.id());
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
    let checked = Wiring::get()
        .enrollment()
        .check(provider, credential_from(provider, values), &panel);

    let _ = app.emit(
        "cli:end",
        Ended {
            job,
            ok: checked.is_ok(),
            message: match &checked {
                Ok(p) => format!("{label} — {}로 확인되었습니다.", p.observation.identity.name()),
                Err(e) => format!("{label} — 실패: {e}"),
            },
        },
    );

    checked.map(into_probe_result).map_err(|e| e.to_string())
}

/// 확인 결과를 화면이 읽을 표현으로 옮긴다. 자격 자체는 넘어가지 않는다.
pub fn into_probe_result(prepared: port::Prepared) -> ProbeResult {
    let identity = &prepared.observation.identity;
    ProbeResult {
        preparation: prepared.id.as_str().to_string(),
        kind: identity.kind().to_string(),
        name: identity.name().to_string(),
        slug: identity.slug(),
        display: identity.display(),
        expires: prepared.observation.facts.expires.clone(),
        scopes: prepared.observation.facts.scopes.clone(),
        git_email: identity.git_email(),
        aws_account_id: identity.aws_account_id(),
        root_keys_present: prepared.observation.facts.root_keys_present,
        root_mfa: prepared.observation.facts.root_mfa,
    }
}

#[tauri::command]
pub fn begin_browser_login(app: AppHandle, provider: String) -> Result<ChallengeResult, String> {
    let provider = account::Provider::parse(&provider)
        .ok_or_else(|| format!("알 수 없는 provider: {provider}"))?;

    let job = next_job_id();
    let label = format!("{} 로그인 시작", provider.id());
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
    let result = Wiring::get().enrollment().begin_browser_login(provider, &panel);
    let _ = app.emit(
        "cli:end",
        Ended {
            job,
            ok: result.is_ok(),
            message: match &result {
                Ok(_) => String::new(),
                Err(e) => format!("{label} — 실패: {e}"),
            },
        },
    );

    let (id, challenge) = result.map_err(|e| e.to_string())?;
    // 방금 받은 주소만 열 수 있게 기억해 둔다.
    remember_auth_url(&challenge.url);
    Ok(ChallengeResult {
        preparation: id.as_str().to_string(),
        url: challenge.url,
        session: challenge.session,
        note: challenge.note,
    })
}

/// 브라우저에서 받은 코드로 로그인을 끝낸다.
#[tauri::command]
pub fn complete_browser_login(
    app: AppHandle,
    preparation: String,
    code: String,
) -> Result<ProbeResult, String> {
    let id = port::PreparationId::named(&preparation);

    let job = next_job_id();
    let label = "로그인 완료".to_string();
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
        .enrollment()
        .complete_browser_login(&id, &secret::Secret::new(code), &panel);
    let _ = app.emit(
        "cli:end",
        Ended {
            job,
            ok: result.is_ok(),
            message: match &result {
                Ok(p) => format!("{label} — {} 로 확인됨", p.observation.identity.name()),
                Err(e) => format!("{label} — 실패: {e}"),
            },
        },
    );

    result.map(into_probe_result).map_err(|e| e.to_string())
}

/// 로그인 중 받은 인증 주소. 그 주소만 열 수 있게 한다.
pub fn remember_auth_url(url: &str) {
    if let Ok(mut slot) = auth_url_slot().lock() {
        *slot = Some(url.to_string());
    }
}

pub fn auth_url_slot() -> &'static std::sync::Mutex<Option<String>> {
    static SLOT: std::sync::OnceLock<std::sync::Mutex<Option<String>>> = std::sync::OnceLock::new();
    SLOT.get_or_init(|| std::sync::Mutex::new(None))
}

/// 값을 얻으러 가야 하는 페이지를 기본 브라우저로 연다.
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    // 레지스트리에 없는 임의 주소를 열지 않는다. 연결 폼이 제공하는 것만 연다.
    let known = account::Provider::ALL
        .iter()
        .filter_map(|p| provider::method(*p).browser)
        .any(|b| b.url == url)
        // 로그인 중 CLI 가 알려 준 인증 주소도 연다. 그 한 건만 허용한다.
        || auth_url_slot()
            .lock()
            .map(|slot| slot.as_deref() == Some(url.as_str()))
            .unwrap_or(false);
    if !known {
        return Err("허용되지 않은 주소입니다".into());
    }

    let open = tools::find_in_path("open").ok_or("open 명령을 찾을 수 없습니다.")?;
    std::process::Command::new(open)
        .arg(&url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("브라우저를 열지 못했습니다: {e}"))
}

/// 확정하지 않기로 한 자격을 버린다.
///
/// 확인만 하고 창을 닫으면 준비 홈에 로그인이 남는다. 자격이 담긴 디렉토리를
/// 방치하지 않기 위해 화면이 물러날 때 이 명령으로 지운다.
#[tauri::command]
pub fn discard_preparation(preparation: String) {
    Wiring::get()
        .enrollment()
        .discard(&port::PreparationId::named(preparation));
}

/// 브라우저 로그인으로 신원을 확인한다.
///
/// 받아 적을 값이 없는 provider 는 로그인 자체가 확인이다. 그 로그인은 준비 홈에
/// 남아 계정을 만들 때 그대로 쓰이므로 브라우저를 두 번 띄우지 않는다.
#[tauri::command]
pub fn probe_browser(app: AppHandle, provider: String) -> Result<ProbeResult, String> {
    let provider = account::Provider::parse(&provider)
        .ok_or_else(|| format!("알 수 없는 provider: {provider}"))?;

    let job = next_job_id();
    let label = format!("{} 브라우저 로그인", provider.id());
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
    let result = Wiring::get().enrollment().check_with_browser(provider, &panel);
    let ok = result.is_ok();
    let _ = app.emit(
        "cli:end",
        Ended {
            job,
            ok,
            message: match &result {
                Ok(p) => format!("{label} — {} 로 확인됨", p.observation.identity.name()),
                Err(e) => format!("{label} — 실패: {e}"),
            },
        },
    );

    result.map(into_probe_result).map_err(|e| e.to_string())
}

