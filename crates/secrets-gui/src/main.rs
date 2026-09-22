// 릴리스 빌드에서 콘솔 창이 함께 뜨지 않게 한다.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use secrets_core::{exec, tools};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// 프론트로 넘기는 표현. core 의 타입을 그대로 노출하지 않고 여기서 한 번 번역한다.
/// 비밀값이 프론트로 새지 않도록 경계를 한 곳으로 모으기 위한 것이다.
#[derive(Serialize)]
struct ToolRow {
    id: String,
    path: Option<String>,
    /// 사람에게 보여줄 설치 방법 한 줄.
    install: String,
    /// 설치 버튼을 달 수 있는가. false 면 안내만 한다.
    installable: bool,
    requirement: String,
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

#[tauri::command]
fn list_tools() -> Vec<ToolRow> {
    tools::inspect_all()
        .into_iter()
        .map(|report| ToolRow {
            id: report.tool.id.to_string(),
            path: report.path.as_ref().map(|p| p.display().to_string()),
            install: report.tool.install.hint(),
            installable: report.tool.install.is_automatic(),
            requirement: describe(report.tool.requirement),
        })
        .collect()
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
        .invoke_handler(tauri::generate_handler![list_tools, install_tool])
        .run(tauri::generate_context!())
        .expect("Tauri 앱 실행 실패");
}
