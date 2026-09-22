//! 비밀값을 담는 그릇.
//!
//! 토큰·액세스 키·인증 코드는 로그·오류 메시지·패닉 출력 어디에도 나오면 안 된다.
//! `Debug` 와 `Display` 를 구현하지 않아, 실수로 찍으려 하면 컴파일이 실패한다.
//! 값을 꺼내려면 [`Secret::expose`] 를 명시적으로 불러야 한다.

/// 밖으로 새면 안 되는 문자열.
///
/// `Debug` 도 `Display` 도 없다. 실수로 찍으려 하면 컴파일이 실패한다.
///
/// ```compile_fail
/// let secret = secrets_core::secret::Secret::new("ghp_비밀");
/// println!("{secret:?}");
/// ```
///
/// ```compile_fail
/// let secret = secrets_core::secret::Secret::new("ghp_비밀");
/// println!("{secret}");
/// ```
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Secret {
        Secret(value.into())
    }

    /// 값을 꺼낸다. 부르는 자리마다 "여기서 비밀이 드러난다"가 보여야 한다.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_only_gives_its_value_when_asked_explicitly() {
        let secret = Secret::new("ghp_비밀");
        assert_eq!(secret.expose(), "ghp_비밀");
    }


    #[test]
    fn blank_input_counts_as_empty() {
        assert!(Secret::new("   ").is_empty());
        assert!(!Secret::new("값").is_empty());
    }
}
