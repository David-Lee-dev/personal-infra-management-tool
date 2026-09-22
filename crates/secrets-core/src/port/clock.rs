//! 지금이 언제인가.
//!
//! 만료 판정과 기록 시각이 여기서 온다. 실제 시각에 묶여 있으면 그 판정을
//! 결정적으로 검사할 수 없다.

/// 지금이 언제인가. 만료 판정과 기록 시각이 여기서 온다.
pub trait Clock: Send + Sync {
    /// `YYYY-MM-DD HH:MM` 형태의 지금.
    fn now(&self) -> String;
    /// `YYYY-MM-DD` 형태의 오늘.
    fn today(&self) -> String;
}
