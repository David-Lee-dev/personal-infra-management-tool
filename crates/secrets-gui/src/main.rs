// 릴리스 빌드에서 콘솔 창이 함께 뜨지 않게 한다.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use secrets_core::{account, active, connect, date, exec, isolation, registration, tools};
use serde::Serialize;
use std::collections::HashMap;
use tauri::{AppHandle, Emitter};

/// 프론트로 넘기는 표현. core 의 타입을 그대로 노출하지 않고 여기서 한 번 번역한다.
/// 비밀값이 프론트로 새지 않도록 경계를 한 곳으로 모으기 위한 것이다.
#[derive(Serialize, Clone)]
struct ToolRow {
    id: String,
    path: Option<String>,
    /// 사람에게 보여줄 설치 방법 한 줄.
    install: String,
    /// 설치 버튼을 달 수 있는가. false 면 안내만 한다.
    installable: bool,
    requirement: String,
    /// 파싱된 버전. 못 읽었으면 None.
    version: Option<String>,
    /// 버전 명령의 원문 첫 줄. 파싱 실패 시 근거로 보여준다.
    version_raw: Option<String>,
    meets_minimum: bool,
    minimum: Option<String>,
    minimum_reason: String,
    /// 컨텍스트별 격리가 가능한가. isolated | leaked | inconclusive | n/a
    isolation: &'static str,
    /// 격리에 쓰는 환경변수.
    isolation_env: String,
    /// 그렇게 판정한 근거.
    isolation_evidence: String,
}

/// 검사 한 판의 결과.
#[derive(Clone, Serialize)]
struct Snapshot {
    tools: Vec<ToolRow>,
    total: usize,
    found: usize,
    /// 쓸 수 없는 툴 — 없거나, 버전이 낮거나, 계정 격리가 깨졌다. 비어 있어야 정상이다.
    blocking: Vec<String>,
    /// 격리를 확인해야 하는 툴 수와 실제로 확인된 수.
    #[serde(rename = "isolationChecked")]
    isolation_checked: usize,
    isolated: usize,
}

#[derive(Clone, Serialize)]
struct Started {
    /// 이 실행을 가리키는 id. 프론트가 완료 이벤트와 짝지을 때 쓴다.
    job: String,
    command: String,
}

#[derive(Clone, Serialize)]
struct Line {
    job: String,
    stream: &'static str,
    line: String,
}

#[derive(Clone, Serialize)]
struct Ended {
    job: String,
    ok: bool,
    message: String,
}

fn row(report: &tools::Report, verdict: &isolation::Verdict) -> ToolRow {
    ToolRow {
        id: report.tool.id.to_string(),
        path: report.path.as_ref().map(|p| p.display().to_string()),
        install: report.tool.install.hint(),
        installable: report.tool.install.is_automatic(),
        requirement: describe(report.tool.requirement),
        version: report.version.as_ref().map(ToString::to_string),
        version_raw: report.version_raw.clone(),
        meets_minimum: report.meets_minimum(),
        minimum: report.tool.minimum.map(str::to_string),
        minimum_reason: report.tool.minimum_reason.to_string(),
        isolation: match verdict.status {
            isolation::Status::Isolated => "isolated",
            isolation::Status::Leaked => "leaked",
            isolation::Status::Inconclusive => "inconclusive",
            isolation::Status::NotApplicable => "n/a",
        },
        isolation_env: verdict.mechanism.clone(),
        isolation_evidence: verdict.evidence.clone(),
    }
}

/// 설치 여부를 훑고 설치된 것마다 버전 명령을 실행한다.
///
/// 버전 명령도 터미널에 그대로 찍힌다. 이 앱이 실행하는 것 중 사용자에게
/// 숨기는 명령은 없다.
#[tauri::command]
fn inspect(app: AppHandle) {
    std::thread::spawn(move || {
        let mut reports = tools::inspect_all();

        for report in &mut reports {
            if !report.found() {
                continue;
            }

            let job = next_job_id();
            let command = exec::display(report.tool.binary, report.tool.version_args);
            let _ = app.emit(
                "cli:start",
                Started {
                    job: job.clone(),
                    command: command.clone(),
                },
            );

            let emitter = app.clone();
            let job_for_lines = job.clone();
            let result = tools::probe_version(report, move |stream, line| {
                let _ = emitter.emit(
                    "cli:line",
                    Line {
                        job: job_for_lines.clone(),
                        stream: match stream {
                            exec::Stream::Stdout => "out",
                            exec::Stream::Stderr => "err",
                        },
                        line,
                    },
                );
            });

            let ok = matches!(&result, Ok(outcome) if outcome.ok());
            let _ = app.emit(
                "cli:end",
                Ended {
                    job,
                    ok,
                    // 버전 확인은 성공해도 조용히 넘어간다. 실패만 눈에 띄면 된다.
                    message: if ok {
                        String::new()
                    } else {
                        format!("{command} — 버전 확인 실패")
                    },
                },
            );
        }

        // 격리 프로브. 계정 스위칭 설계 전체가 이 결과 위에 있으므로 매번 실측한다.
        let verdicts: Vec<isolation::Verdict> = reports
            .iter()
            .map(|report| {
                if !report.found()
                    || isolation::mechanism_of(report.tool.id)
                        == isolation::Mechanism::NotApplicable
                {
                    return isolation::probe(report, |_, _| {});
                }

                let job = next_job_id();
                let command = format!(
                    "{}= {} …",
                    isolation::describe(&isolation::mechanism_of(report.tool.id)),
                    report.tool.binary
                );
                let _ = app.emit(
                    "cli:start",
                    Started {
                        job: job.clone(),
                        command: command.clone(),
                    },
                );

                let emitter = app.clone();
                let job_for_lines = job.clone();
                let verdict = isolation::probe(report, move |stream, line| {
                    let _ = emitter.emit(
                        "cli:line",
                        Line {
                            job: job_for_lines.clone(),
                            stream: match stream {
                                exec::Stream::Stdout => "out",
                                exec::Stream::Stderr => "err",
                            },
                            line,
                        },
                    );
                });

                let ok = verdict.status == isolation::Status::Isolated;
                let _ = app.emit(
                    "cli:end",
                    Ended {
                        job,
                        ok,
                        message: if ok {
                            String::new()
                        } else {
                            format!("{} — {}", report.tool.id, verdict.evidence)
                        },
                    },
                );
                verdict
            })
            .collect();

        let tools: Vec<ToolRow> = reports
            .iter()
            .zip(&verdicts)
            .map(|(report, verdict)| row(report, verdict))
            .collect();
        let snapshot = Snapshot {
            total: tools.len(),
            found: reports.iter().filter(|r| r.found()).count(),
            blocking: reports
                .iter()
                .zip(&verdicts)
                .filter(|(report, verdict)| report.blocks() || isolation::blocks(verdict))
                .map(|(report, _)| report.tool.id.to_string())
                .collect(),
            isolation_checked: verdicts
                .iter()
                .filter(|v| v.status != isolation::Status::NotApplicable)
                .count(),
            isolated: verdicts
                .iter()
                .filter(|v| v.status == isolation::Status::Isolated)
                .count(),
            tools,
        };
        let _ = app.emit("tools:updated", snapshot);
    });
}

/// 레지스트리에 박힌 설치 명령을 실행한다.
///
/// 프론트에서 받은 문자열을 실행하지 않는다. id 로 레지스트리를 찾아 거기 적힌
/// program/args 만 쓴다.
#[tauri::command]
fn install_tool(app: AppHandle, id: String) -> Result<String, String> {
    let tool = tools::find(&id).ok_or_else(|| format!("알 수 없는 툴: {id}"))?;

    let tools::Install::Command { program, args } = tool.install else {
        return Err(format!("{id} 는 자동 설치를 지원하지 않습니다"));
    };

    // PATH 를 여기서 직접 풀어 둔다. Finder 로 띄운 앱은 PATH 가 빈약해
    // brew/npm 을 못 찾는 경우가 있다.
    let program_path =
        tools::find_in_path(program).ok_or_else(|| format!("{program} 을 찾을 수 없습니다"))?;

    Ok(spawn_cli(app, &program_path, program, args))
}

/// CLI 실행을 띄우고 출력을 터미널 패널로 흘린다.
///
/// 앞으로 계정 추가·검증 등 모든 외부 명령이 이 함수를 거친다.
/// 실행되는 모든 것이 사용자에게 보이도록 통로를 하나로 유지한다.
fn spawn_cli(
    app: AppHandle,
    program_path: &std::path::Path,
    program: &'static str,
    args: &'static [&'static str],
) -> String {
    let job = next_job_id();
    let command = exec::display(program, args);

    let _ = app.emit(
        "cli:start",
        Started {
            job: job.clone(),
            command: command.clone(),
        },
    );

    let program_path = program_path.to_path_buf();
    let job_for_thread = job.clone();

    // 설치는 수 분이 걸릴 수 있으므로 별도 스레드에서 돌리고 창은 계속 살아 있게 한다.
    std::thread::spawn(move || {
        let emitter = app.clone();
        let job_for_lines = job_for_thread.clone();

        let result = exec::run(&program_path, args, move |stream, line| {
            let _ = emitter.emit(
                "cli:line",
                Line {
                    job: job_for_lines.clone(),
                    stream: match stream {
                        exec::Stream::Stdout => "out",
                        exec::Stream::Stderr => "err",
                    },
                    line,
                },
            );
        });

        let ended = match result {
            Ok(outcome) if outcome.ok() => Ended {
                job: job_for_thread,
                ok: true,
                message: format!("{command} — 완료"),
            },
            Ok(outcome) => Ended {
                job: job_for_thread,
                ok: false,
                message: format!(
                    "{command} — 실패 (종료 코드 {})",
                    outcome.code.unwrap_or(-1)
                ),
            },
            Err(e) => Ended {
                job: job_for_thread,
                ok: false,
                message: format!("{command} — 실행 실패: {e}"),
            },
        };
        let _ = app.emit("cli:end", ended);
    });

    job
}

fn next_job_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    format!("job-{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// 프론트로 넘기는 계정 표현.
#[derive(Serialize)]
struct AccountRow {
    slug: String,
    provider: &'static str,
    display: String,
    note: String,
    identity_kind: String,
    identity_name: String,
    /// 이 계정 전용 CLI 설정 홈. 격리의 실체라 사용자가 볼 수 있어야 한다.
    cli_home: String,
    verified_at: Option<String>,
    verified_ok: Option<bool>,
    verified_detail: Option<String>,
    /// 만료일 (`YYYY-MM-DD`). 적지 않았으면 None.
    expires: Option<String>,
    /// unset | ok | soon | expired
    expiry: &'static str,
    /// soon 이면 남은 일수, expired 면 지난 일수.
    expiry_days: Option<i64>,
    /// 만료됐을 때 무엇을 해야 하는가. 자격 종류마다 다르다.
    renewal_hint: &'static str,
    /// 이 자격이 가진 권한.
    scopes: Vec<String>,
    /// 지난 자격 교체 횟수.
    replacements: usize,
    /// 지금 전역으로 활성화된 계정인가.
    is_active: bool,
    /// 전역 전환으로 갈아끼울 자리. 지원하지 않으면 None.
    global_path: Option<String>,
    /// 전역 전환이 다른 도구에 영향을 줄 수 있으면 그 이유.
    caution: Option<&'static str>,
    /// 이 계정으로 커밋할 때 쓸 이메일.
    git_email: Option<String>,
    /// AWS 계정 번호.
    aws_account_id: Option<String>,
    /// root 에 액세스 키가 있는가.
    root_keys_present: Option<bool>,
    /// root 에 MFA 가 걸려 있는가.
    root_mfa: Option<bool>,
}

#[derive(Serialize)]
struct AccountList {
    accounts: Vec<AccountRow>,
    /// 읽지 못한 항목. 조용히 숨기면 계정이 사라진 것처럼 보인다.
    errors: Vec<String>,
    /// 만료가 임박했거나 지난 계정. 어느 탭에 있든 상시로 알린다.
    alerts: Vec<String>,
}

#[tauri::command]
fn list_accounts() -> AccountList {
    let mut accounts = Vec::new();
    let mut errors = Vec::new();

    for entry in account::list() {
        match entry {
            Ok(acc) => accounts.push(AccountRow {
                slug: acc.slug.clone(),
                provider: acc.provider.id(),
                display: acc.display.clone(),
                note: acc.note.clone(),
                identity_kind: acc.identity.kind.clone(),
                identity_name: acc.identity.name.clone(),
                cli_home: acc.cli_home().display().to_string(),
                verified_at: acc.verification.as_ref().map(|v| v.checked_at.clone()),
                verified_ok: acc.verification.as_ref().map(|v| v.ok),
                verified_detail: acc.verification.as_ref().map(|v| v.detail.clone()),
                expires: acc.expires.clone(),
                expiry: match acc.expiry() {
                    account::Expiry::Unset => "unset",
                    account::Expiry::Never => "never",
                    account::Expiry::Ok => "ok",
                    account::Expiry::Soon(_) => "soon",
                    account::Expiry::Expired(_) => "expired",
                },
                expiry_days: match acc.expiry() {
                    account::Expiry::Soon(d) | account::Expiry::Expired(d) => Some(d),
                    _ => None,
                },
                renewal_hint: acc.renewal_hint(),
                scopes: acc.scopes.clone(),
                replacements: acc.history().len(),
                is_active: active::is_active(&acc),
                global_path: active::link_for(&acc).map(|l| l.global.display().to_string()),
                caution: active::caution(acc.provider),
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

#[derive(Serialize)]
struct FieldSpec {
    key: &'static str,
    label: &'static str,
    secret: bool,
    help: &'static str,
    required: bool,
}

#[derive(Serialize)]
struct FormSpec {
    fields: Vec<FieldSpec>,
    guidance: &'static str,
    browser_label: Option<&'static str>,
    browser_url: Option<&'static str>,
    /// 이 provider 를 다루는 CLI 가 설치돼 있는가.
    tool_ready: bool,
    tool: &'static str,
    /// 입력 대신 브라우저 로그인으로 연결하는가.
    browser_login: bool,
    /// 브라우저에서 받은 코드를 되돌려 넣어야 끝나는가.
    browser_code: bool,
}

/// provider 를 연결하려면 무엇을 입력받아야 하는가.
#[tauri::command]
fn provider_form(provider: String) -> Result<FormSpec, String> {
    let provider = account::Provider::parse(&provider)
        .ok_or_else(|| format!("알 수 없는 provider: {provider}"))?;
    let method = connect::method(provider);

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
        tool_ready: tools::find_in_path(provider.tool()).is_some(),
        tool: provider.tool(),
        browser_login: method.browser_login,
        browser_code: method.browser_code,
    })
}

/// 입력한 자격으로 신원을 미리 읽어 온다.
///
/// 계정을 만들기 전에 임시 홈에서 돌린다. 이름과 만료일을 사람이 추측해 적는
/// 대신 자격 자체에서 읽어 오기 위한 것이다.
#[derive(Serialize)]
struct ProbeResult {
    /// 확인이 끝난 자격을 가리키는 표. 계정을 만들 때 이것만 되돌려 보낸다.
    preparation: String,
    kind: String,
    name: String,
    slug: String,
    display: String,
    expires: Option<String>,
    scopes: Vec<String>,
    git_email: Option<String>,
    aws_account_id: Option<String>,
    root_keys_present: Option<bool>,
    root_mfa: Option<bool>,
}

#[tauri::command]
fn probe_credentials(
    provider: String,
    values: HashMap<String, String>,
) -> Result<ProbeResult, String> {
    let provider = account::Provider::parse(&provider)
        .ok_or_else(|| format!("알 수 없는 provider: {provider}"))?;

    registration::prepare(provider, &values, |_, _| {})
        .map(|(id, probe)| into_probe_result(&id, probe))
        .map_err(|e| e.to_string())
}

fn into_probe_result(id: &registration::PreparationId, probe: connect::Probe) -> ProbeResult {
    ProbeResult {
        preparation: id.as_str().to_string(),
        kind: probe.kind,
        name: probe.name,
        slug: probe.slug,
        display: probe.display,
        expires: probe.expires,
        scopes: probe.scopes,
        git_email: probe.git_email,
        aws_account_id: probe.aws_account_id,
        root_keys_present: probe.root_keys_present,
        root_mfa: probe.root_mfa,
    }
}

/// 코드를 받아 와야 끝나는 로그인을 시작한다.
#[derive(Serialize)]
struct ChallengeResult {
    /// 두 번째 단계가 같은 로그인을 가리키게 하는 표.
    preparation: String,
    url: String,
    /// 브라우저 페이지에서 대조할 세션 번호.
    session: String,
    note: String,
}

#[tauri::command]
fn begin_browser_login(app: AppHandle, provider: String) -> Result<ChallengeResult, String> {
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

    let result = registration::begin_browser_login(provider, line_emitter(&app, &job));
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
fn complete_browser_login(
    app: AppHandle,
    preparation: String,
    code: String,
) -> Result<ProbeResult, String> {
    let id = registration::PreparationId::named(&preparation);

    let job = next_job_id();
    let label = "로그인 완료".to_string();
    let _ = app.emit(
        "cli:start",
        Started {
            job: job.clone(),
            command: label.clone(),
        },
    );

    let result = registration::complete_browser_login(&id, &code, line_emitter(&app, &job));
    let _ = app.emit(
        "cli:end",
        Ended {
            job,
            ok: result.is_ok(),
            message: match &result {
                Ok(p) => format!("{label} — {} 로 확인됨", p.name),
                Err(e) => format!("{label} — 실패: {e}"),
            },
        },
    );

    result
        .map(|probe| into_probe_result(&id, probe))
        .map_err(|e| e.to_string())
}

/// 로그인 중 받은 인증 주소. 그 주소만 열 수 있게 한다.
fn remember_auth_url(url: &str) {
    if let Ok(mut slot) = auth_url_slot().lock() {
        *slot = Some(url.to_string());
    }
}

fn auth_url_slot() -> &'static std::sync::Mutex<Option<String>> {
    static SLOT: std::sync::OnceLock<std::sync::Mutex<Option<String>>> = std::sync::OnceLock::new();
    SLOT.get_or_init(|| std::sync::Mutex::new(None))
}

/// 값을 얻으러 가야 하는 페이지를 기본 브라우저로 연다.
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    // 레지스트리에 없는 임의 주소를 열지 않는다. 연결 폼이 제공하는 것만 연다.
    let known = account::Provider::ALL
        .iter()
        .filter_map(|p| connect::method(*p).browser)
        .any(|b| b.url == url)
        // 로그인 중 CLI 가 알려 준 인증 주소도 연다. 그 한 건만 허용한다.
        || auth_url_slot()
            .lock()
            .map(|slot| slot.as_deref() == Some(url.as_str()))
            .unwrap_or(false);
    if !known {
        return Err("허용되지 않은 주소입니다".into());
    }

    let open = tools::find_in_path("open").ok_or("open 을 찾을 수 없습니다")?;
    std::process::Command::new(open)
        .arg(&url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("브라우저를 열지 못했습니다: {e}"))
}

/// 계정을 만들고 입력값으로 연결한 뒤 실제 신원을 확인한다.
///
/// 입력값은 여기서 CLI 로 넘어갈 뿐, 파일에 저장되지 않는다.
/// 저장되는 건 CLI 가 자기 설정 홈에 쓴 것뿐이다.
/// 폼이 보내는 계정 정보. 인자를 늘어놓는 대신 한 덩이로 받는다.
/// 화면이 보내오는 것. 확인 단계가 읽어 온 사실은 여기 없다.
///
/// 신원·권한·만료일을 화면에서 받아 적으면 화면이 그 값을 고쳐 보낼 수 있다.
/// 그런 값은 준비 표가 가리키는 관찰 결과에서만 온다.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewAccount {
    /// 자격 확인이 돌려준 표.
    preparation: String,
    slug: String,
    #[serde(default)]
    display: String,
    #[serde(default)]
    note: String,
}

/// 확인된 자격을 계정으로 확정한다.
#[tauri::command]
fn create_account(app: AppHandle, account: NewAccount) -> Result<(), String> {
    let NewAccount {
        preparation,
        slug,
        display,
        note,
    } = account;

    let id = registration::PreparationId::named(&preparation);
    let draft = registration::Draft {
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

    let made = registration::commit(&id, draft);
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

/// 확정하지 않기로 한 자격을 버린다.
///
/// 확인만 하고 창을 닫으면 준비 홈에 로그인이 남는다. 자격이 담긴 디렉토리를
/// 방치하지 않기 위해 화면이 물러날 때 이 명령으로 지운다.
#[tauri::command]
fn discard_preparation(preparation: String) {
    registration::discard(&registration::PreparationId::named(&preparation));
}

/// 브라우저 로그인으로 신원을 확인한다.
///
/// 받아 적을 값이 없는 provider 는 로그인 자체가 확인이다. 그 로그인은 준비 홈에
/// 남아 계정을 만들 때 그대로 쓰이므로 브라우저를 두 번 띄우지 않는다.
#[tauri::command]
fn probe_browser(app: AppHandle, provider: String) -> Result<ProbeResult, String> {
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

    let result = registration::prepare_with_browser(provider, line_emitter(&app, &job));
    let ok = result.is_ok();
    let _ = app.emit(
        "cli:end",
        Ended {
            job,
            ok,
            message: match &result {
                Ok((_, p)) => format!("{label} — {} 로 확인됨", p.name),
                Err(e) => format!("{label} — 실패: {e}"),
            },
        },
    );

    result
        .map(|(id, probe)| into_probe_result(&id, probe))
        .map_err(|e| e.to_string())
}

/// 이 계정을 전역으로 활성화한다.
///
/// 자리에 있던 실물은 지우지 않고 보관소로 옮긴다.
#[tauri::command]
fn activate_account(app: AppHandle, provider: String, slug: String) -> Result<(), String> {
    let provider = account::Provider::parse(&provider)
        .ok_or_else(|| format!("알 수 없는 provider: {provider}"))?;
    let acc = account::load(provider, &slug).map_err(|e| format!("계정을 읽지 못했습니다: {e}"))?;

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

    let result = active::activate(&acc);
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
fn archive_account(app: AppHandle, provider: String, slug: String) -> Result<(), String> {
    let provider = account::Provider::parse(&provider)
        .ok_or_else(|| format!("알 수 없는 provider: {provider}"))?;
    let acc = account::load(provider, &slug).map_err(|e| format!("계정을 읽지 못했습니다: {e}"))?;

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
    if active::is_active(&acc) {
        if let Err(e) = active::deactivate(provider) {
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

    let result = account::archive_account(provider, &slug, account::ArchiveReason::Deleted);
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
fn deactivate_provider(app: AppHandle, provider: String) -> Result<(), String> {
    let provider = account::Provider::parse(&provider)
        .ok_or_else(|| format!("알 수 없는 provider: {provider}"))?;
    active::deactivate(provider).map_err(|e| e.to_string())?;
    let _ = app.emit("accounts:updated", ());
    Ok(())
}

/// 확인된 새 자격으로 이 계정의 자격을 교체한다.
///
/// 확인·교체·기록·저장 중 어디서 실패하든 계정은 손대기 전 상태로 남는다.
/// 실패한 교체는 이력에도 남지 않는다.
#[tauri::command]
fn replace_credential(
    app: AppHandle,
    provider: String,
    slug: String,
    preparation: String,
) -> Result<(), String> {
    let provider = account::Provider::parse(&provider)
        .ok_or_else(|| format!("알 수 없는 provider: {provider}"))?;
    let acc = account::load(provider, &slug).map_err(|e| format!("계정을 읽지 못했습니다: {e}"))?;

    let job = next_job_id();
    let label = format!("{}/{} 자격 교체", provider.id(), slug);
    let _ = app.emit(
        "cli:start",
        Started {
            job: job.clone(),
            command: label.clone(),
        },
    );

    let detail = if matches!(acc.expiry(), account::Expiry::Expired(_)) {
        "만료되어 교체"
    } else {
        "기한 전 교체"
    };

    let id = registration::PreparationId::named(&preparation);
    let done = registration::replace(&id, &acc, detail);

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
fn verify_account(app: AppHandle, provider: String, slug: String) -> Result<(), String> {
    let provider = account::Provider::parse(&provider)
        .ok_or_else(|| format!("알 수 없는 provider: {provider}"))?;
    let mut acc =
        account::load(provider, &slug).map_err(|e| format!("계정을 읽지 못했습니다: {e}"))?;

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

        let (ok, message) = match connect::verify(&acc, line_emitter(&app, &job)) {
            Ok(whoami) => {
                acc.identity.kind = whoami.kind.clone();
                acc.identity.name = whoami.name.clone();
                acc.verification = Some(account::Verification {
                    checked_at: date::now(),
                    ok: whoami.ok,
                    detail: whoami.detail.clone(),
                });
                let _ = acc.save();
                if whoami.ok {
                    (true, format!("{label} — {} 로 확인됨", whoami.name))
                } else {
                    (false, format!("{label} — {}", whoami.detail))
                }
            }
            Err(e) => (false, format!("{label} — 실패: {e}")),
        };

        let _ = app.emit("cli:end", Ended { job, ok, message });
        let _ = app.emit("accounts:updated", ());
    });

    Ok(())
}

/// CLI 출력을 터미널로 흘리는 클로저.
fn line_emitter(
    app: &AppHandle,
    job: &str,
) -> impl Fn(exec::Stream, String) + Send + Sync + 'static {
    let app = app.clone();
    let job = job.to_string();
    move |stream, line| {
        let _ = app.emit(
            "cli:line",
            Line {
                job: job.clone(),
                stream: match stream {
                    exec::Stream::Stdout => "out",
                    exec::Stream::Stderr => "err",
                },
                line,
            },
        );
    }
}

fn describe(requirement: tools::Requirement) -> String {
    match requirement {
        tools::Requirement::Base => "필수".to_string(),
        tools::Requirement::WhenAccount(p) => format!("{p} 계정 등록 시"),
        tools::Requirement::WhenFeature(f) => format!("{f} 기능 사용 시"),
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            inspect,
            install_tool,
            list_accounts,
            provider_form,
            probe_credentials,
            discard_preparation,
            probe_browser,
            begin_browser_login,
            complete_browser_login,
            open_url,
            create_account,
            verify_account,
            replace_credential,
            activate_account,
            deactivate_provider,
            archive_account
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 앱 실행 실패");
}
