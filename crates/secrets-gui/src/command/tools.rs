//! 환경 구성 — 이 머신에 필요한 CLI 가 갖춰져 있는가.

use secrets_local::cli::{exec, tools};
use secrets_local::isolation;
use tauri::{AppHandle, Emitter};

use crate::dto::*;
use crate::progress::*;

pub fn row(report: &tools::Report, verdict: &isolation::Verdict) -> ToolRow {
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
pub fn inspect(app: AppHandle) {
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
pub fn install_tool(app: AppHandle, id: String) -> Result<String, String> {
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
pub fn spawn_cli(
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

fn describe(requirement: tools::Requirement) -> String {
    match requirement {
        tools::Requirement::Base => "필수".to_string(),
        tools::Requirement::WhenAccount(p) => format!("{p} 계정 등록 시"),
        tools::Requirement::WhenFeature(f) => format!("{f} 기능 사용 시"),
    }
}
