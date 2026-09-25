//! 스캔에 쓰는 git 질의. 전부 읽기 전용이고, `-C` 로 그 디렉토리에서만 실행한다.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use secrets_core::project::GitState;

use crate::cli::{exec, tools};

/// 한 번의 git 실행 결과 — 성공 여부와 stdout 줄.
struct Answer {
    ok: bool,
    lines: Vec<String>,
}

pub struct Git {
    program: PathBuf,
}

impl Git {
    pub fn find() -> Option<Git> {
        tools::find_in_path("git").map(|program| Git { program })
    }

    fn ask(&self, dir: &Path, args: &[&str]) -> Option<Answer> {
        let dir_text = dir.display().to_string();
        let mut full = vec!["-C", dir_text.as_str()];
        full.extend_from_slice(args);

        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = lines.clone();
        let outcome = exec::run(&self.program, &full, move |stream, line| {
            if stream == exec::Stream::Stdout
                && let Ok(mut all) = sink.lock()
            {
                all.push(line);
            }
        })
        .ok()?;
        let lines = lines.lock().ok()?.clone();
        Some(Answer {
            ok: outcome.ok(),
            lines,
        })
    }

    /// 실행이 성공했을 때의 첫 줄.
    fn first_line(&self, dir: &Path, args: &[&str]) -> Option<String> {
        self.ask(dir, args)
            .filter(|a| a.ok)
            .and_then(|a| a.lines.into_iter().next())
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
    }

    /// 이 디렉토리가 저장소의 뿌리일 때만 상태를 읽는다. 상위 저장소에 들어 있는
    /// 하위 디렉토리는 그 자체로 저장소가 아니다.
    pub fn state(&self, dir: &Path) -> GitState {
        let Some(top) = self.first_line(dir, &["rev-parse", "--show-toplevel"]) else {
            return GitState::Absent;
        };
        if !same_directory(Path::new(&top), dir) {
            return GitState::Absent;
        }

        let branch = self.first_line(dir, &["symbolic-ref", "--quiet", "--short", "HEAD"]);
        let commits = self
            .first_line(dir, &["rev-list", "--count", "HEAD"])
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        let changes = self
            .ask(dir, &["status", "--porcelain"])
            .filter(|a| a.ok)
            .map(|a| a.lines.iter().filter(|l| !l.trim().is_empty()).count() as u32)
            .unwrap_or(0);

        match self.first_line(dir, &["remote", "get-url", "origin"]) {
            Some(origin) => GitState::Remote {
                branch,
                commits,
                changes,
                origin,
            },
            None => GitState::Local {
                branch,
                commits,
                changes,
            },
        }
    }

    /// 커밋의 전체 sha 와 제목. 브랜치 · 원격 브랜치 · 줄인 sha 모두 받는다.
    pub fn revision(&self, dir: &Path, reference: &str) -> Option<(String, String)> {
        let target = format!("{reference}^{{commit}}");
        let sha = self.first_line(dir, &["rev-parse", "--verify", "--quiet", &target])?;
        let subject = self
            .first_line(dir, &["log", "-1", "--format=%s", &sha])
            .unwrap_or_default();
        Some((sha, subject))
    }

    /// `from..to` 의 커밋 수.
    pub fn count(&self, dir: &Path, from: &str, to: &str) -> Option<u32> {
        let range = format!("{from}..{to}");
        self.first_line(dir, &["rev-list", "--count", &range])?
            .parse()
            .ok()
    }

    pub fn tracks(&self, dir: &Path, file: &str) -> bool {
        self.ask(dir, &["ls-files", "--error-unmatch", "--", file])
            .is_some_and(|a| a.ok)
    }

    /// 제외 규칙에 걸리는가. 판단할 수 없으면 `None`.
    pub fn ignores(&self, dir: &Path, file: &str) -> Option<bool> {
        // check-ignore 는 걸리면 0, 안 걸리면 1, 오류면 128 로 끝난다.
        let dir_text = dir.display().to_string();
        let outcome = exec::run(
            &self.program,
            &["-C", dir_text.as_str(), "check-ignore", "-q", "--", file],
            |_, _| {},
        )
        .ok()?;
        match outcome.code {
            Some(0) => Some(true),
            Some(1) => Some(false),
            _ => None,
        }
    }

    /// 이 레포의 `core.sshCommand` 가 `-i` 로 가리키는 파일.
    pub fn ssh_key(&self, dir: &Path) -> Option<String> {
        let command = self.first_line(dir, &["config", "--local", "--get", "core.sshCommand"])?;
        identity_file(&command)
    }

    /// 쓰는 명령. 실패하면 stderr 를 이유로 돌려준다.
    pub fn write(&self, dir: &Path, args: &[&str]) -> Result<(), String> {
        let dir_text = dir.display().to_string();
        let mut full = vec!["-C", dir_text.as_str()];
        full.extend_from_slice(args);
        let errors = Arc::new(Mutex::new(Vec::new()));
        let sink = errors.clone();
        let outcome = exec::run(&self.program, &full, move |stream, line| {
            if stream == exec::Stream::Stderr
                && let Ok(mut all) = sink.lock()
            {
                all.push(line);
            }
        })
        .map_err(|e| e.to_string())?;
        if outcome.ok() {
            return Ok(());
        }
        Err(errors.lock().map(|e| e.join(" ")).unwrap_or_default())
    }

    /// `origin` 에 닿는지 본다. 출력은 그대로 흘린다 — 실패하면 사람이 이유를 봐야 한다.
    pub fn reach(
        &self,
        dir: &Path,
        on_line: impl Fn(exec::Stream, String) + Sync,
    ) -> Result<(), String> {
        let dir_text = dir.display().to_string();
        let outcome = exec::run(
            &self.program,
            &["-C", dir_text.as_str(), "ls-remote", "--heads", "origin"],
            on_line,
        )
        .map_err(|e| e.to_string())?;
        if outcome.ok() {
            Ok(())
        } else {
            Err(format!(
                "origin에 접속하지 못했습니다 (git 종료 코드 {})",
                outcome.code.unwrap_or(-1)
            ))
        }
    }

    pub fn init(&self, dir: &Path) -> Result<(), String> {
        let dir_text = dir.display().to_string();
        let errors = Arc::new(Mutex::new(Vec::new()));
        let sink = errors.clone();
        let outcome = exec::run(
            &self.program,
            &["-C", dir_text.as_str(), "init", "--initial-branch=main"],
            move |stream, line| {
                if stream == exec::Stream::Stderr
                    && let Ok(mut all) = sink.lock()
                {
                    all.push(line);
                }
            },
        )
        .map_err(|e| e.to_string())?;
        if outcome.ok() {
            return Ok(());
        }
        let detail = errors.lock().map(|e| e.join(" ")).unwrap_or_default();
        Err(detail)
    }
}

/// 심볼릭 링크(`/tmp` → `/private/tmp` 같은)를 풀어서 비교한다.
fn same_directory(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// `ssh -i <파일> -o IdentitiesOnly=yes` 에서 파일. 따옴표로 감싼 경로도 읽는다.
pub fn identity_file(command: &str) -> Option<String> {
    let words = shell_words(command);
    let mut rest = words.iter();
    while let Some(word) = rest.next() {
        if word == "-i" {
            return rest.next().cloned();
        }
        if let Some(path) = word.strip_prefix("-i") {
            return Some(path.to_string());
        }
    }
    None
}

/// 공백으로 나누되 작은따옴표 · 큰따옴표 안은 한 낱말로 둔다. git 이 이 값을 셸에 넘기기 때문이다.
fn shell_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in text.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started || !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
                started = false;
            }
            (None, c) => current.push(c),
        }
    }
    if started || !current.is_empty() {
        words.push(current);
    }
    words
}

/// `core.sshCommand` 에 적을 값. 경로는 작은따옴표로 감싼다.
/// 작은따옴표가 든 경로는 받지 않는다 — 셸로 넘어가는 값이라 안전하게 감쌀 수 없다.
pub fn ssh_command(private_key: &str) -> Option<String> {
    (!private_key.contains('\'')).then(|| format!("ssh -i '{private_key}' -o IdentitiesOnly=yes"))
}

#[cfg(test)]
mod tests {
    use super::identity_file;

    #[test]
    fn reads_the_identity_file_in_its_usual_shapes() {
        assert_eq!(
            identity_file("ssh -i /v/key -o IdentitiesOnly=yes").as_deref(),
            Some("/v/key")
        );
        assert_eq!(
            identity_file("ssh -i '/v/my key' -o X=1").as_deref(),
            Some("/v/my key")
        );
        assert_eq!(identity_file("ssh -i/v/key").as_deref(), Some("/v/key"));
        assert_eq!(identity_file("ssh -o IdentitiesOnly=yes"), None);
    }

    #[test]
    fn the_command_it_writes_reads_back_to_the_same_path() {
        for path in ["/v/key", "/v/my key"] {
            let command = super::ssh_command(path).unwrap();
            assert_eq!(identity_file(&command).as_deref(), Some(path), "{path}");
        }
        assert_eq!(super::ssh_command("/v/it's"), None);
    }
}
