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

#[derive(Debug, Clone, Copy)]
pub struct Tool {
    /// 레지스트리 키이자 표시 이름.
    pub id: &'static str,
    /// PATH 에서 찾을 실행 파일 이름.
    pub binary: &'static str,
    pub requirement: Requirement,
    /// 없을 때 사용자에게 보여줄 설치 명령.
    pub install: &'static str,
}

/// 툴 추가는 이 배열에 한 줄을 넣는 것으로 끝난다.
pub const REGISTRY: &[Tool] = &[
    Tool {
        id: "gh",
        binary: "gh",
        requirement: Requirement::Base,
        install: "brew install gh",
    },
    Tool {
        id: "aws",
        binary: "aws",
        requirement: Requirement::Base,
        install: "brew install awscli",
    },
    Tool {
        id: "git",
        binary: "git",
        requirement: Requirement::Base,
        install: "xcode-select --install",
    },
    Tool {
        id: "ssh",
        binary: "ssh",
        requirement: Requirement::Base,
        install: "macOS 기본 제공",
    },
    Tool {
        id: "gcloud",
        binary: "gcloud",
        requirement: Requirement::WhenAccount("gcloud"),
        install: "brew install --cask google-cloud-sdk",
    },
    Tool {
        id: "firebase",
        binary: "firebase",
        requirement: Requirement::WhenAccount("firebase"),
        install: "npm i -g firebase-tools",
    },
    Tool {
        id: "age",
        binary: "age",
        requirement: Requirement::WhenFeature("backup"),
        install: "brew install age",
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
