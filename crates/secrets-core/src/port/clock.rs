//! 지금이 언제인가.
//!
//! 만료 판정과 기록 시각이 여기서 온다. 실제 시각에 묶여 있으면 그 판정을
//! 결정적으로 검사할 수 없다.

/// 지금이 언제인가. 만료 판정과 기록 시각이 여기서 온다.
pub trait Clock: Send + Sync {
    /// ISO 8601 UTC 로 적은 지금. 검증·교체 시각 기록에 쓴다.
    fn now(&self) -> String;
    /// `YYYY-MM-DD` 형태의 오늘.
    fn today(&self) -> String;
}
