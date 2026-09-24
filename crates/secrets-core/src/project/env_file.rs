//! 환경 변수 파일의 역할. 로컬 파일이 정본이고, 환경마다 파일 하나가 대응한다.
//!
//! ```text
//! .env · .env.local                  local
//! .env.<환경>                        그 환경 (서버로 반영하는 원본)
//! .env.example · .sample · .template 예시 — 값이 없어야 하는 파일
//! ```

use serde::Serialize;

use super::scan::EnvFileFact;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "env", rename_all = "snake_case")]
pub enum EnvFileRole {
    Example,
    Local,
    Environment(String),
    /// 규칙에 맞지 않는 이름. 값이 있을 수 있는 파일로 다룬다.
    Other,
}

const EXAMPLES: &[&str] = &["example", "sample", "template", "dist"];

/// 파일 이름으로 역할을 정한다.
pub fn role(name: &str) -> EnvFileRole {
    if name == ".env" || name == ".env.local" {
        return EnvFileRole::Local;
    }
    let Some(suffix) = name.strip_prefix(".env.") else {
        return EnvFileRole::Other;
    };
    if EXAMPLES.contains(&suffix) {
        return EnvFileRole::Example;
    }
    let plain = !suffix.is_empty()
        && suffix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if plain {
        EnvFileRole::Environment(suffix.to_string())
    } else {
        EnvFileRole::Other
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvFileView {
    pub name: String,
    pub role: EnvFileRole,
    pub variables: usize,
    pub tracked: bool,
    pub ignored: Option<bool>,
}

impl EnvFileView {
    /// 값이 담길 파일인데 git 이 올릴 수 있는 상태다 — 추적 중이거나 제외 규칙이 없다.
    ///
    /// git 저장소가 아니면 제외 여부를 알 수 없으므로 추적 중일 때만 노출로 본다.
    pub fn exposed(&self) -> bool {
        if self.role == EnvFileRole::Example {
            return false;
        }
        self.tracked || self.ignored == Some(false)
    }
}

pub fn view(fact: &EnvFileFact) -> EnvFileView {
    EnvFileView {
        name: fact.name.clone(),
        role: role(&fact.name),
        variables: fact.variables,
        tracked: fact.tracked,
        ignored: fact.ignored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod role {
        use super::*;

        #[test]
        fn dot_env_and_dot_env_local_are_the_local_environment() {
            assert_eq!(role(".env"), EnvFileRole::Local);
            assert_eq!(role(".env.local"), EnvFileRole::Local);
        }

        #[test]
        fn a_plain_suffix_names_an_environment() {
            assert_eq!(role(".env.prod"), EnvFileRole::Environment("prod".into()));
            assert_eq!(role(".env.dev"), EnvFileRole::Environment("dev".into()));
            assert_eq!(role(".env.staging-2"), EnvFileRole::Environment("staging-2".into()));
        }

        #[test]
        fn example_suffixes_are_examples() {
            for name in [".env.example", ".env.sample", ".env.template", ".env.dist"] {
                assert_eq!(role(name), EnvFileRole::Example, "{name}");
            }
        }

        #[test]
        fn anything_else_is_other() {
            assert_eq!(role(".env.prod.local"), EnvFileRole::Other);
            assert_eq!(role(".envrc"), EnvFileRole::Other);
            assert_eq!(role(".env."), EnvFileRole::Other);
        }
    }

    mod exposed {
        use super::*;

        fn file(name: &str, tracked: bool, ignored: Option<bool>) -> EnvFileView {
            view(&EnvFileFact {
                name: name.into(),
                variables: 3,
                tracked,
                ignored,
            })
        }

        #[test]
        fn a_value_file_that_git_would_pick_up_is_exposed() {
            assert!(file(".env.local", false, Some(false)).exposed());
        }

        #[test]
        fn a_tracked_value_file_is_exposed_even_if_ignored_later() {
            assert!(file(".env.prod", true, Some(true)).exposed());
        }

        #[test]
        fn an_ignored_untracked_value_file_is_safe() {
            assert!(!file(".env.local", false, Some(true)).exposed());
        }

        #[test]
        fn outside_a_repository_only_tracking_counts() {
            assert!(!file(".env.local", false, None).exposed());
        }

        #[test]
        fn an_example_is_never_exposed() {
            assert!(!file(".env.example", true, Some(false)).exposed());
        }
    }
}
