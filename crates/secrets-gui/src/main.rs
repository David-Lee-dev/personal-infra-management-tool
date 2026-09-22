// 릴리스 빌드에서 콘솔 창이 함께 뜨지 않게 한다.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use secrets_core::{exec, isolation, tools};
use serde::Serialize;
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

fn describe(requirement: tools::Requirement) -> String {
    match requirement {
        tools::Requirement::Base => "필수".to_string(),
        tools::Requirement::WhenAccount(p) => format!("{p} 계정 등록 시"),
        tools::Requirement::WhenFeature(f) => format!("{f} 기능 사용 시"),
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![inspect, install_tool])
        .run(tauri::generate_context!())
        .expect("Tauri 앱 실행 실패");
}
