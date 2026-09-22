//! 비밀값을 담는 그릇.
//!
//! 토큰·액세스 키·인증 코드는 로그·오류 메시지·패닉 출력 어디에도 나오면 안 된다.
//! `Debug` 와 `Display` 를 구현하지 않아, 실수로 찍으려 하면 컴파일이 실패한다.
//! 값을 꺼내려면 [`Secret::expose`] 를 명시적으로 불러야 한다.

/// 밖으로 새면 안 되는 문자열.
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
    fn a_secret_does_not_render_itself() {
        let secret = Secret::new("ghp_비밀");
        // 값은 꺼내야만 나온다.
        assert_eq!(secret.expose(), "ghp_비밀");
        // 포맷 문자열에 넣을 수 없다는 것이 이 타입의 요점이다.
        assert!(!format!("{:?}", secret.is_empty()).contains("ghp_"));
    }

    #[test]
    fn blank_input_counts_as_empty() {
        assert!(Secret::new("   ").is_empty());
        assert!(!Secret::new("값").is_empty());
    }
}
