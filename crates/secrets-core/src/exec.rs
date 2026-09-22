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
/// `on_line` 은 stdout·stderr 양쪽에서 호출되므로 스레드 안전해야 한다.
pub fn run<F>(program: &Path, args: &[&str], on_line: F) -> std::io::Result<Outcome>
where
    F: Fn(Stream, String) + Send + Sync + 'static,
{
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let on_line = std::sync::Arc::new(on_line);
    let stdout = child
        .stdout
        .take()
        .map(|s| Box::new(s) as Box<dyn Read + Send>);
    let stderr = child
        .stderr
        .take()
        .map(|s| Box::new(s) as Box<dyn Read + Send>);

    // 두 스트림을 동시에 읽어야 한다. 한쪽만 읽으면 다른 쪽 파이프가 차서 교착한다.
    let out = pump(stdout, Stream::Stdout, on_line.clone());
    let err = pump(stderr, Stream::Stderr, on_line);

    let status = child.wait()?;
    let _ = out.join();
    let _ = err.join();

    Ok(Outcome {
        code: status.code(),
    })
}

fn pump<F>(
    reader: Option<Box<dyn Read + Send>>,
    stream: Stream,
    on_line: std::sync::Arc<F>,
) -> std::thread::JoinHandle<()>
where
    F: Fn(Stream, String) + Send + Sync + 'static,
{
    std::thread::spawn(move || {
        let Some(reader) = reader else { return };
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            on_line(stream, line);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn captures_both_streams_and_exit_code() {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = lines.clone();

        let sh = crate::tools::find_in_path("sh").expect("sh 가 있어야 한다");
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
    fn display_joins_args() {
        assert_eq!(display("brew", &["install", "age"]), "brew install age");
        assert_eq!(display("gh", &[]), "gh");
    }
}
