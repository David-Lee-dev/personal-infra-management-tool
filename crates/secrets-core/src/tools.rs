//! 툴 레지스트리와 PATH 탐색.
//!
//! "이 CLI 를 돌릴 수 있는가" 만 답한다. "누구로 돌아가는가" 는 account 의 몫이다.

use std::path::PathBuf;

/// 언제 이 툴이 필요해지는가. Phase 5 에서 등록된 계정을 보고 실제 등급으로 해석된다.
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
    },
    Tool {
        id: "aws",
        binary: "aws",
        requirement: Requirement::Base,
        install: Install::Command {
            program: "brew",
            args: &["install", "awscli"],
        },
    },
    Tool {
        id: "git",
        binary: "git",
        requirement: Requirement::Base,
        install: Install::Manual("xcode-select --install 을 터미널에서 직접 실행"),
    },
    Tool {
        id: "ssh",
        binary: "ssh",
        requirement: Requirement::Base,
        install: Install::Manual("macOS 기본 제공"),
    },
    Tool {
        id: "gcloud",
        binary: "gcloud",
        requirement: Requirement::WhenAccount("gcloud"),
        install: Install::Command {
            program: "brew",
            args: &["install", "--cask", "google-cloud-sdk"],
        },
    },
    Tool {
        id: "firebase",
        binary: "firebase",
        requirement: Requirement::WhenAccount("firebase"),
        install: Install::Command {
            program: "npm",
            args: &["install", "-g", "firebase-tools"],
        },
    },
    Tool {
        id: "age",
        binary: "age",
        requirement: Requirement::WhenFeature("backup"),
        install: Install::Command {
            program: "brew",
            args: &["install", "age"],
        },
    },
];

/// 한 툴에 대한 검사 결과. phase 가 진행되며 필드가 늘어난다.
#[derive(Debug, Clone)]
pub struct Report {
    pub tool: &'static Tool,
    /// PATH 에서 찾은 실행 파일. 없으면 None.
    pub path: Option<PathBuf>,
}

impl Report {
    pub fn found(&self) -> bool {
        self.path.is_some()
    }
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
