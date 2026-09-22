//! 외부 CLI 실행의 단일 통로.
//!
//! 이 도구가 하는 일은 결국 CLI 를 대신 실행해 주는 것이다. 그래서 실행 경로를
//! 하나로 모으고, 무엇을 실행했고 무엇이 나왔는지를 호출자가 전부 관찰할 수 있게 한다.
//! GUI 는 이 관찰 지점을 터미널 패널에 그대로 흘리고, CLI 는 stdout 에 흘린다.
//!
//! 셸을 거치지 않는다. program 과 args 를 분리해 받으므로 인용·이스케이프 문제가
//! 없고, 문자열 하나를 실행하는 경로가 애초에 존재하지 않는다.

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};

/// 출력이 어느 스트림에서 나왔는가.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// 실행 결과.
#[derive(Debug, Clone, Copy)]
pub struct Outcome {
    pub code: Option<i32>,
}

impl Outcome {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }
}

/// 사람이 읽을 수 있는 명령 한 줄. 터미널 패널의 `$ ...` 행에 쓴다.
pub fn display(program: &str, args: &[&str]) -> String {
    if args.is_empty() {
        program.to_string()
    } else {
        format!("{program} {}", args.join(" "))
    }
}

/// 명령을 실행하고 출력을 줄 단위로 흘린다. 프로세스가 끝날 때까지 블록한다.
///
/// `on_line` 은 stdout·stderr 양쪽 스레드에서 불리므로 `Sync` 여야 한다.
/// 수명은 요구하지 않는다 — 읽기 스레드가 이 호출 안에서 시작하고 끝난다.
pub fn run<F>(program: &Path, args: &[&str], on_line: F) -> std::io::Result<Outcome>
where
    F: Fn(Stream, String) + Sync,
{
    run_env(program, args, &[], on_line)
}

/// 환경변수를 덧씌워 실행한다.
///
/// CLI 격리는 전부 환경변수로 이뤄지므로 이 함수가 격리의 실행 지점이다.
/// 부모 환경을 지우지 않고 덧씌우기만 한다 — PATH 같은 건 그대로 필요하다.
pub fn run_env<F>(
    program: &Path,
    args: &[&str],
    env: &[(&str, String)],
    on_line: F,
) -> std::io::Result<Outcome>
where
    F: Fn(Stream, String) + Sync,
{
    run_full(program, args, env, None, on_line)
}

/// 표준 입력으로 값을 넣어 실행한다.
///
/// 비밀값은 **반드시** 이 경로로 넘긴다. 명령행 인자는 같은 머신의 다른 프로세스가
/// `ps` 로 그대로 읽을 수 있고, 셸 히스토리에도 남는다. stdin 은 그렇지 않다.
/// 이 함수는 넘긴 값을 로그로 흘리지 않는다 — 출력만 `on_line` 으로 간다.
pub fn run_full<F>(
    program: &Path,
    args: &[&str],
    env: &[(&str, String)],
    stdin_data: Option<&[u8]>,
    on_line: F,
) -> std::io::Result<Outcome>
where
    F: Fn(Stream, String) + Sync,
{
    let mut child = Command::new(program)
        .args(args)
        .envs(env.iter().map(|(k, v)| (*k, v.as_str())))
        .stdin(if stdin_data.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // 값을 넘기고 파이프를 닫는다. 닫지 않으면 CLI 가 입력을 더 기다리며 멈춘다.
    if let Some(data) = stdin_data
        && let Some(mut pipe) = child.stdin.take()
    {
        use std::io::Write;
        pipe.write_all(data)?;
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let on_line = &on_line;

    // 두 스트림을 동시에 읽어야 한다. 한쪽만 읽으면 다른 쪽 파이프가 차서 교착한다.
    // 읽기 스레드가 이 범위 안에서 끝나므로 호출자의 관찰자를 그대로 빌려 쓴다.
    let status = std::thread::scope(|scope| {
        scope.spawn(move || pump(stdout, Stream::Stdout, on_line));
        scope.spawn(move || pump(stderr, Stream::Stderr, on_line));
        child.wait()
    })?;

    Ok(Outcome {
        code: status.code(),
    })
}

fn pump<R, F>(reader: Option<R>, stream: Stream, on_line: &F)
where
    R: Read,
    F: Fn(Stream, String) + Sync,
{
    let Some(reader) = reader else { return };
    for line in BufReader::new(reader).lines().map_while(Result::ok) {
        on_line(stream, line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn captures_both_streams_and_exit_code() {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = lines.clone();

        let sh = crate::cli::tools::find_in_path("sh").expect("sh 가 있어야 한다");
        let outcome = run(
            &sh,
            &["-c", "echo 나온다; echo 오류 1>&2; exit 3"],
            move |s, l| {
                sink.lock().unwrap().push((s, l));
            },
        )
        .unwrap();

        assert_eq!(outcome.code, Some(3));
        assert!(!outcome.ok());

        let captured = lines.lock().unwrap();
        assert!(captured.contains(&(Stream::Stdout, "나온다".to_string())));
        assert!(captured.contains(&(Stream::Stderr, "오류".to_string())));
    }

    #[test]
    fn feeds_stdin_without_logging_it() {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = lines.clone();

        let sh = crate::cli::tools::find_in_path("sh").expect("sh 가 있어야 한다");
        let outcome = run_full(
            &sh,
            &["-c", "read value; test \"$value\" = 비밀 && echo 일치"],
            &[],
            Some("비밀\n".as_bytes()),
            move |s, l| sink.lock().unwrap().push((s, l)),
        )
        .unwrap();

        assert!(outcome.ok(), "stdin 이 전달되지 않았다");
        let captured = lines.lock().unwrap();
        assert!(captured.contains(&(Stream::Stdout, "일치".to_string())));
        // 넘긴 값 자체는 어디에도 찍히지 않는다.
        assert!(!captured.iter().any(|(_, l)| l.contains("비밀")));
    }

    #[test]
    fn display_joins_args() {
        assert_eq!(display("brew", &["install", "age"]), "brew install age");
        assert_eq!(display("gh", &[]), "gh");
    }
}
