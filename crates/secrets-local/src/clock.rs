//! 이 머신의 시계.
//!
//! core 는 지금이 언제인지 스스로 알지 못한다. 그 사실이 들어오는 자리가 여기다.

use secrets_core::time;

/// 오늘 (UTC).
pub fn today() -> String {
    from_now().0
}

/// 지금 (UTC, ISO 8601). 검증 시각과 교체 시각 기록에 쓴다.
pub fn now() -> String {
    from_now().1
}

fn from_now() -> (String, String) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    time::from_unix_seconds(secs)
}
