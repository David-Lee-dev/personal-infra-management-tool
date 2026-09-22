// 릴리스 빌드에서 콘솔 창이 함께 뜨지 않게 한다.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use secrets_core::tools;
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
struct LogLine {
    id: String,
    line: String,
}

#[derive(Clone, Serialize)]
struct Done {
    id: String,
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

/// 레지스트리에 박힌 설치 명령을 실행하고 출력을 줄 단위로 프론트에 흘린다.
///
/// 프론트에서 받은 문자열을 실행하지 않는다. id 로 레지스트리를 찾아 거기 적힌
/// program/args 만 쓰고, 셸도 거치지 않는다.
#[tauri::command]
fn install_tool(app: AppHandle, id: String) -> Result<(), String> {
    let tool = tools::find(&id).ok_or_else(|| format!("알 수 없는 툴: {id}"))?;

    let tools::Install::Command { program, args } = tool.install else {
        return Err(format!("{id} 는 자동 설치를 지원하지 않습니다"));
    };

    // PATH 를 여기서 직접 풀어 둔다. Finder 로 띄운 앱은 PATH 가 빈약해
    // brew/npm 을 못 찾는 경우가 있다.
    let program_path =
        tools::find_in_path(program).ok_or_else(|| format!("{program} 을 찾을 수 없습니다"))?;

    let mut child = Command::new(&program_path)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{program} 실행 실패: {e}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // 설치는 수 분이 걸릴 수 있으므로 별도 스레드에서 돌리고 창은 계속 살아 있게 한다.
    std::thread::spawn(move || {
        let pump = |reader: Option<Box<dyn std::io::Read + Send>>, app: AppHandle, id: String| {
            std::thread::spawn(move || {
                let Some(reader) = reader else { return };
                for line in BufReader::new(reader).lines().map_while(Result::ok) {
                    let _ = app.emit(
                        "install:log",
                        LogLine {
                            id: id.clone(),
                            line,
                        },
                    );
                }
            })
        };

        let out = pump(
            stdout.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
            app.clone(),
            id.clone(),
        );
        let err = pump(
            stderr.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
            app.clone(),
            id.clone(),
        );

        let status = child.wait();
        let _ = out.join();
        let _ = err.join();

        let done = match status {
            Ok(s) if s.success() => Done {
                id,
                ok: true,
                message: "설치 완료".into(),
            },
            Ok(s) => Done {
                id,
                ok: false,
                message: format!("설치 실패 (종료 코드 {})", s.code().unwrap_or(-1)),
            },
            Err(e) => Done {
                id,
                ok: false,
                message: format!("설치 실패: {e}"),
            },
        };
        let _ = app.emit("install:done", done);
    });

    Ok(())
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
