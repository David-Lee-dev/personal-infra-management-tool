//! 이 머신의 시계.

use secrets_core::date;

/// 오늘 (UTC).
pub fn today() -> String {
    from_now().0
}

/// 지금 (UTC, ISO 8601). 검증 시각 기록에 쓴다.
pub fn now() -> String {
    from_now().1
}

fn from_now() -> (String, String) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    date::from_unix_seconds(secs)
}
