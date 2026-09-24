//! 프로젝트 이름 · 그룹 · 디렉토리 이름의 규칙.

use super::ProjectError;

/// 프로젝트 이름. 기록의 디렉토리 이름이 되므로 경로 조각으로 안전한 글자만 받는다.
pub fn check_name(text: &str) -> Result<String, ProjectError> {
    let name = text.trim();
    let valid = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if valid {
        Ok(name.to_string())
    } else {
        Err(ProjectError::Invalid(
            "이름에는 영문 · 숫자 · - · _ · .만 쓸 수 있고, .으로 시작할 수 없습니다.".into(),
        ))
    }
}

/// 만들 디렉토리의 이름. 경로 구분자와 `.` · `..` 만 막는다.
pub fn check_directory(text: &str) -> Result<String, ProjectError> {
    let name = text.trim();
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        return Err(ProjectError::Invalid(
            "디렉토리 이름을 확인하세요. /는 쓸 수 없습니다.".into(),
        ));
    }
    Ok(name.to_string())
}

pub fn check_group(text: &str) -> Result<String, ProjectError> {
    let group = text.trim();
    if group.is_empty() || group.contains('/') {
        return Err(ProjectError::Invalid("그룹을 입력하세요. /는 쓸 수 없습니다.".into()));
    }
    Ok(group.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    mod check_name {
        use super::*;

        #[test]
        fn keeps_a_plain_name_trimmed() {
            assert_eq!(check_name("  ledger-lite ").unwrap(), "ledger-lite");
        }

        #[test]
        fn rejects_path_pieces_and_hidden_names() {
            for bad in ["", "a/b", "..", ".env", "한글", "a b"] {
                assert!(check_name(bad).is_err(), "{bad}");
            }
        }
    }

    mod check_directory {
        use super::*;

        #[test]
        fn allows_any_name_without_a_separator() {
            assert_eq!(check_directory("09_가계부").unwrap(), "09_가계부");
        }

        #[test]
        fn rejects_separators_and_dot_names() {
            for bad in ["", ".", "..", "a/b"] {
                assert!(check_directory(bad).is_err(), "{bad}");
            }
        }
    }
}
