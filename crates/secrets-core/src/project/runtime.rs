//! 런타임 판정. 프로젝트 파일에서 찾은 근거만으로 정하고, 근거가 없으면 모른다고 한다.
//!
//! 근거 파일을 읽고 해석하는 일은 어댑터가 한다. 여기서는 근거들을 모아 무엇을 설치할 수
//! 있는지, 근거끼리 맞지 않는지를 정한다.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Runtime {
    Node,
    Python,
    Rust,
    Go,
    Ruby,
    Dart,
    Java,
    Pnpm,
    Npm,
    Yarn,
    Bun,
    Uv,
    Poetry,
}

impl Runtime {
    pub fn label(self) -> &'static str {
        match self {
            Runtime::Node => "Node.js",
            Runtime::Python => "Python",
            Runtime::Rust => "Rust",
            Runtime::Go => "Go",
            Runtime::Ruby => "Ruby",
            Runtime::Dart => "Dart",
            Runtime::Java => "Java",
            Runtime::Pnpm => "pnpm",
            Runtime::Npm => "npm",
            Runtime::Yarn => "Yarn",
            Runtime::Bun => "Bun",
            Runtime::Uv => "uv",
            Runtime::Poetry => "Poetry",
        }
    }

    /// 언어 런타임이 아니라 패키지 관리자인가.
    pub fn is_package_manager(self) -> bool {
        matches!(
            self,
            Runtime::Pnpm | Runtime::Npm | Runtime::Yarn | Runtime::Bun | Runtime::Uv | Runtime::Poetry
        )
    }
}

/// 파일 하나가 말해 주는 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEvidence {
    pub runtime: Runtime,
    /// 파일에 적힌 버전. 존재만 알려 주는 파일이면 없다.
    pub version: Option<String>,
    /// 뿌리 기준 파일 이름과, 필요하면 그 안의 항목 (`package.json packageManager`).
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedRuntime {
    pub runtime: Runtime,
    /// 가장 구체적인 버전. 근거가 맞지 않으면 없다.
    pub version: Option<String>,
    pub sources: Vec<String>,
    /// 근거끼리 버전이 맞지 않는다. 설치하지 않고 사람에게 묻는다.
    pub conflict: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeVerdict {
    pub detected: Vec<DetectedRuntime>,
}

impl RuntimeVerdict {
    pub fn from_evidence(evidence: &[RuntimeEvidence]) -> RuntimeVerdict {
        let mut runtimes: Vec<Runtime> = evidence.iter().map(|e| e.runtime).collect();
        runtimes.sort();
        runtimes.dedup();

        let detected = runtimes
            .into_iter()
            .map(|runtime| Self::resolve(runtime, evidence))
            .collect();
        RuntimeVerdict { detected }
    }

    /// 서버에 설치할 수 있는가 — 언어 런타임이 하나 이상 있고 맞지 않는 근거가 없다.
    pub fn installable(&self) -> bool {
        let has_language = self.detected.iter().any(|d| !d.runtime.is_package_manager());
        has_language && !self.detected.iter().any(|d| d.conflict)
    }

    fn resolve(runtime: Runtime, evidence: &[RuntimeEvidence]) -> DetectedRuntime {
        let mine: Vec<&RuntimeEvidence> = evidence.iter().filter(|e| e.runtime == runtime).collect();
        let versions: Vec<Vec<u32>> = mine
            .iter()
            .filter_map(|e| e.version.as_deref())
            .filter_map(numbers)
            .collect();

        let conflict = versions
            .iter()
            .enumerate()
            .any(|(i, a)| versions[i + 1..].iter().any(|b| !agree(a, b)));
        let version = if conflict {
            None
        } else {
            versions
                .iter()
                .max_by_key(|v| v.len())
                .map(|v| v.iter().map(u32::to_string).collect::<Vec<_>>().join("."))
        };
        DetectedRuntime {
            runtime,
            version,
            sources: mine.iter().map(|e| e.source.clone()).collect(),
            conflict,
        }
    }
}

/// `v22.3.0` · `3.12` · `1.79.0` 같은 표기에서 숫자 마디만. 숫자가 없으면 버전이 아니다.
fn numbers(text: &str) -> Option<Vec<u32>> {
    let trimmed = text.trim().trim_start_matches(['v', 'V']);
    let parts: Vec<u32> = trimmed
        .split('.')
        .map_while(|part| {
            let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        })
        .collect();
    (!parts.is_empty()).then_some(parts)
}

/// 짧은 쪽이 긴 쪽의 앞부분이면 같은 버전을 가리킨다 (`22` 와 `22.3.0`).
fn agree(a: &[u32], b: &[u32]) -> bool {
    a.iter().zip(b).all(|(x, y)| x == y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(runtime: Runtime, version: Option<&str>, source: &str) -> RuntimeEvidence {
        RuntimeEvidence {
            runtime,
            version: version.map(str::to_string),
            source: source.into(),
        }
    }

    mod from_evidence {
        use super::*;

        #[test]
        fn agreeing_versions_merge_into_the_most_specific_one() {
            let verdict = RuntimeVerdict::from_evidence(&[
                found(Runtime::Node, Some("22"), ".nvmrc"),
                found(Runtime::Node, Some("v22.3.0"), ".node-version"),
                found(Runtime::Node, None, "package.json"),
            ]);
            let node = &verdict.detected[0];
            assert_eq!(node.version.as_deref(), Some("22.3.0"));
            assert_eq!(node.sources, vec![".nvmrc", ".node-version", "package.json"]);
            assert!(!node.conflict);
        }

        #[test]
        fn disagreeing_versions_are_a_conflict_without_a_version() {
            let verdict = RuntimeVerdict::from_evidence(&[
                found(Runtime::Python, Some("3.12"), ".python-version"),
                found(Runtime::Python, Some("3.11.4"), "runtime.txt"),
            ]);
            assert!(verdict.detected[0].conflict);
            assert_eq!(verdict.detected[0].version, None);
            assert!(!verdict.installable());
        }

        #[test]
        fn presence_only_evidence_detects_without_a_version() {
            let verdict = RuntimeVerdict::from_evidence(&[found(Runtime::Rust, None, "Cargo.toml")]);
            assert_eq!(verdict.detected[0].version, None);
            assert!(verdict.installable());
        }

        #[test]
        fn a_version_without_digits_is_ignored() {
            let verdict = RuntimeVerdict::from_evidence(&[
                found(Runtime::Rust, Some("stable"), "rust-toolchain.toml"),
                found(Runtime::Rust, Some("1.79"), "Cargo.toml rust-version"),
            ]);
            assert!(!verdict.detected[0].conflict);
            assert_eq!(verdict.detected[0].version.as_deref(), Some("1.79"));
        }

        #[test]
        fn several_runtimes_are_listed_in_a_fixed_order() {
            let verdict = RuntimeVerdict::from_evidence(&[
                found(Runtime::Pnpm, Some("9.1.0"), "package.json packageManager"),
                found(Runtime::Node, Some("22"), ".nvmrc"),
            ]);
            let order: Vec<Runtime> = verdict.detected.iter().map(|d| d.runtime).collect();
            assert_eq!(order, vec![Runtime::Node, Runtime::Pnpm]);
        }
    }

    mod installable {
        use super::*;

        #[test]
        fn nothing_found_is_not_installable() {
            assert!(!RuntimeVerdict::from_evidence(&[]).installable());
        }

        #[test]
        fn a_package_manager_alone_is_not_installable() {
            let verdict = RuntimeVerdict::from_evidence(&[found(Runtime::Pnpm, None, "pnpm-lock.yaml")]);
            assert!(!verdict.installable());
        }
    }
}
