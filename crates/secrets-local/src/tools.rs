//! 툴 레지스트리와 PATH 탐색.
//!
//! "이 CLI 를 돌릴 수 있는가" 만 답한다. "누구로 돌아가는가" 는 account 의 몫이다.

use secrets_core::version::Version;
use std::path::PathBuf;

/// 언제 이 툴이 필요해지는가.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    /// 이 도구의 어느 기능을 쓰든 필요하다.
    Base,
    /// 해당 provider 계정이 하나라도 등록되면 필요해진다.
    WhenAccount(&'static str),
    /// 해당 기능을 쓸 때 필요해진다.
    WhenFeature(&'static str),
}

/// 이 툴을 어떻게 설치하는가.
///
/// 명령을 문자열 한 줄로 두지 않고 program + args 로 쪼갠 건, GUI 의 설치 버튼이
/// 셸을 거치지 않고 직접 실행하기 위해서다. 셸을 끼우면 임의 문자열 실행 경로가
/// 생기므로 레지스트리에 박힌 인자만 넘어가도록 강제한다.
#[derive(Debug, Clone, Copy)]
pub enum Install {
    Command {
        program: &'static str,
        args: &'static [&'static str],
    },
    /// 자동 설치가 불가능하거나 부적절하다. 안내 문구만 보여준다.
    Manual(&'static str),
}

impl Install {
    /// 사람에게 보여줄 한 줄.
    pub fn hint(&self) -> String {
        match self {
            Install::Command { program, args } => format!("{program} {}", args.join(" ")),
            Install::Manual(text) => (*text).to_string(),
        }
    }

    pub fn is_automatic(&self) -> bool {
        matches!(self, Install::Command { .. })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Tool {
    /// 레지스트리 키이자 표시 이름.
    pub id: &'static str,
    /// PATH 에서 찾을 실행 파일 이름.
    pub binary: &'static str,
    pub requirement: Requirement,
    /// 없을 때 어떻게 설치하는가.
    pub install: Install,
    /// 버전을 묻는 인자. 대부분 `--version` 이지만 ssh 처럼 다른 것도 있다.
    pub version_args: &'static [&'static str],
    /// 요구 최소 버전. 근거가 없으면 두지 않는다 — 임의의 하한은 거짓 경고만 만든다.
    pub minimum: Option<&'static str>,
    /// 그 최소 버전이 왜 필요한지. 사용자에게 그대로 보여준다.
    pub minimum_reason: &'static str,
}

/// 툴 추가는 이 배열에 한 줄을 넣는 것으로 끝난다.
pub const REGISTRY: &[Tool] = &[
    Tool {
        id: "gh",
        binary: "gh",
        requirement: Requirement::Base,
        install: Install::Command {
            program: "brew",
            args: &["install", "gh"],
        },
        version_args: &["--version"],
        minimum: Some("2.40"),
        minimum_reason: "gh auth switch 가 2.40 에 도입됐다",
    },
    Tool {
        id: "aws",
        binary: "aws",
        requirement: Requirement::Base,
        install: Install::Command {
            program: "brew",
            args: &["install", "awscli"],
        },
        version_args: &["--version"],
        minimum: Some("2.0"),
        minimum_reason: "v1 은 sso·프로필 동작이 달라 지원하지 않는다",
    },
    Tool {
        id: "git",
        binary: "git",
        requirement: Requirement::Base,
        install: Install::Manual("xcode-select --install 을 터미널에서 직접 실행"),
        version_args: &["--version"],
        minimum: None,
        minimum_reason: "",
    },
    Tool {
        id: "ssh",
        binary: "ssh",
        requirement: Requirement::Base,
        install: Install::Manual("macOS 기본 제공"),
        version_args: &["-V"],
        minimum: None,
        minimum_reason: "",
    },
    Tool {
        id: "gcloud",
        binary: "gcloud",
        requirement: Requirement::WhenAccount("gcloud"),
        install: Install::Command {
            program: "brew",
            args: &["install", "--cask", "google-cloud-sdk"],
        },
        version_args: &["--version"],
        minimum: None,
        minimum_reason: "",
    },
    Tool {
        id: "firebase",
        binary: "firebase",
        requirement: Requirement::WhenAccount("firebase"),
        install: Install::Command {
            program: "npm",
            args: &["install", "-g", "firebase-tools"],
        },
        version_args: &["--version"],
        minimum: None,
        minimum_reason: "",
    },
    Tool {
        id: "gpg",
        binary: "gpg",
        requirement: Requirement::WhenFeature("gpg-key"),
        install: Install::Command {
            program: "brew",
            args: &["install", "gnupg"],
        },
        version_args: &["--version"],
        minimum: None,
        minimum_reason: "",
    },
    Tool {
        id: "age",
        binary: "age",
        requirement: Requirement::WhenFeature("backup"),
        install: Install::Command {
            program: "brew",
            args: &["install", "age"],
        },
        version_args: &["--version"],
        minimum: Some("1.0"),
        minimum_reason: "1.0 이전은 파일 포맷이 호환되지 않는다",
    },
];

/// 한 툴에 대한 검사 결과.
#[derive(Debug, Clone)]
pub struct Report {
    pub tool: &'static Tool,
    /// PATH 에서 찾은 실행 파일. 없으면 None.
    pub path: Option<PathBuf>,
    /// 버전 검사를 돌렸다면 그 결과. 아직 안 돌렸으면 None.
    pub version: Option<Version>,
    /// 버전 명령이 뱉은 원문 첫 줄. 파싱에 실패했을 때 사용자에게 보여준다.
    pub version_raw: Option<String>,
}

impl Report {
    pub fn found(&self) -> bool {
        self.path.is_some()
    }

    /// 최소 버전 요구를 만족하는가.
    ///
    /// 요구가 없으면 만족으로 본다. 버전을 못 읽은 경우도 만족으로 본다 —
    /// 파싱 실패를 버전 미달로 취급하면 멀쩡한 툴을 막게 된다.
    pub fn meets_minimum(&self) -> bool {
        let Some(minimum) = self.tool.minimum else {
            return true;
        };
        let (Some(actual), Some(required)) = (&self.version, Version::parse(minimum)) else {
            return true;
        };
        *actual >= required
    }

    /// 이 툴 때문에 전체 검사가 실패해야 하는가.
    ///
    /// `Base` 만 본다. 특정 계정·기능에만 필요한 툴은 그 기능을 쓸 때 막는다.
    pub fn blocks(&self) -> bool {
        matches!(self.tool.requirement, Requirement::Base)
            && (!self.found() || !self.meets_minimum())
    }
}

/// 버전 명령을 실행해 Report 를 채운다.
///
/// 출력은 `on_line` 으로도 흘려보내 호출자가 터미널에 그대로 보여줄 수 있게 한다.
pub fn probe_version<F>(report: &mut Report, on_line: F) -> std::io::Result<crate::exec::Outcome>
where
    F: Fn(crate::exec::Stream, String) + Sync,
{
    let Some(path) = report.path.clone() else {
        return Ok(crate::exec::Outcome { code: None });
    };

    // 버전은 stdout 과 stderr 어느 쪽으로도 나온다. ssh -V 는 stderr 로 뱉는다.
    let collected = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = collected.clone();

    let outcome = crate::exec::run(&path, report.tool.version_args, move |stream, line| {
        if let Ok(mut buf) = sink.lock() {
            buf.push_str(&line);
            buf.push('\n');
        }
        on_line(stream, line);
    })?;

    let text = collected.lock().map(|b| b.clone()).unwrap_or_default();
    report.version = Version::from_output(&text);
    report.version_raw = text.lines().next().map(str::to_string);
    Ok(outcome)
}

/// id 로 레지스트리 항목을 찾는다.
pub fn find(id: &str) -> Option<&'static Tool> {
    REGISTRY.iter().find(|t| t.id == id)
}

/// 레지스트리 전체를 검사한다.
pub fn inspect_all() -> Vec<Report> {
    REGISTRY
        .iter()
        .map(|tool| Report {
            tool,
            path: find_in_path(tool.binary),
            version: None,
            version_raw: None,
        })
        .collect()
}

/// PATH 를 앞에서부터 훑어 실행 가능한 파일을 찾는다.
///
/// `which` 크레이트를 쓰지 않는 건 의존성을 줄이려는 것이고, 이 정도 로직이면 충분하다.
pub fn find_in_path(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(binary))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    // 디렉토리가 같은 이름으로 있을 수 있으므로 파일 여부까지 본다.
    match std::fs::metadata(path) {
        Ok(meta) => meta.is_file() && meta.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ids_are_unique() {
        let mut ids: Vec<_> = REGISTRY.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "레지스트리에 중복된 id 가 있다");
    }

    #[test]
    fn finds_a_binary_that_must_exist() {
        // POSIX 시스템에 sh 가 없을 수는 없다.
        assert!(find_in_path("sh").is_some());
    }

    #[test]
    fn missing_binary_is_none() {
        assert!(find_in_path("secrets-no-such-binary-xyz").is_none());
    }
}
