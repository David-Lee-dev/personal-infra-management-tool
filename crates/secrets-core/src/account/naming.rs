//! 계정 이름 규칙.
//!
//! 슬러그는 디렉토리 이름이 된다. 경로를 벗어나는 값이 들어오면 계정 하나가 남의
//! 자리를 가리키게 되므로 엄격하게 막는다.

/// 슬러그 규칙. 경로가 되므로 엄격하게 막는다.
pub fn validate_slug(slug: &str) -> Result<(), String> {
    if slug.is_empty() {
        return Err("이름을 입력하세요.".into());
    }
    if slug.len() > 48 {
        return Err("이름은 48자 이하여야 합니다.".into());
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("소문자, 숫자, 하이픈만 사용할 수 있습니다.".into());
    }
    if slug.starts_with('-') || slug.ends_with('-') {
        return Err("이름은 하이픈으로 시작하거나 끝날 수 없습니다.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_slug_must_be_usable_as_a_directory_name() {
        assert!(validate_slug("tuk-prod").is_ok());
        assert!(validate_slug("").is_err());
        assert!(validate_slug("Tuk").is_err(), "대문자는 막는다");
        assert!(validate_slug("tuk prod").is_err(), "공백은 막는다");
        assert!(validate_slug("-tuk").is_err());
        assert!(validate_slug("tuk-").is_err());
        assert!(validate_slug(&"a".repeat(49)).is_err());
    }

}
