//! 이 머신의 시계.
//!
//! core 는 지금이 언제인지도, 여기가 어느 시간대인지도 스스로 알지 못한다.
//! 그 두 사실이 들어오는 자리가 여기다.
//!
//! 기록은 **이 머신의 벽시계**로 적는다. UTC 로 적으면 한국에서 새벽에 만든 키가
//! 전날로 찍혀, 사용자가 자기 달력과 대조할 때 하루 어긋나 보인다.

use secrets_core::time;

/// 오늘.
pub fn today() -> String {
    from_now().0
}

/// 지금. ISO 8601 에 시간대까지 적는다.
pub fn now() -> String {
    from_now().1
}

/// 디렉토리 이름으로 쓰는 시각. `20260923-060712`.
///
/// 이력과 보관 디렉토리가 시각순으로 정렬되어야 하므로 구분자를 최소로 둔다.
pub fn stamp() -> String {
    let (date, time) = from_now();
    let clock: String = time
        .chars()
        .skip_while(|c| *c != 'T')
        .skip(1)
        .take(8)
        .filter(char::is_ascii_digit)
        .collect();
    format!("{}-{clock}", date.replace('-', ""))
}

fn from_now() -> (String, String) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    time::from_unix_seconds(secs, offset(secs))
}

/// UTC 에서 이 머신까지의 초.
///
/// 표준 라이브러리는 시간대를 모른다. 이 값을 아는 것은 OS 뿐이라 거기에 묻는다.
/// 서머타임 때문에 시점마다 달라질 수 있으므로 그 시점을 넘겨 묻는다.
#[cfg(unix)]
fn offset(secs: i64) -> i32 {
    let when = secs as libc::time_t;
    let mut parts: libc::tm = unsafe { std::mem::zeroed() };

    // SAFETY: `when` 은 유효한 time_t 이고 `parts` 는 우리가 소유한 온전한 tm 이다.
    // localtime_r 은 전역 상태를 건드리지 않아 스레드 사이에서 안전하다.
    let filled = unsafe { libc::localtime_r(&when, &mut parts) };
    if filled.is_null() {
        // 시간대를 읽지 못하면 UTC 로 적는다. 틀린 오프셋을 적는 것보다 낫다.
        return 0;
    }
    parts.tm_gmtoff as i32
}

#[cfg(not(unix))]
fn offset(_secs: i64) -> i32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_offset_is_a_real_time_zone() {
        // ±14 시간을 넘는 시간대는 지구에 없다. 쓰레기 값을 읽고 있으면 여기서 걸린다.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let seconds = offset(now);
        assert!((-14 * 3600..=14 * 3600).contains(&seconds), "{seconds}");
        // 15 분 단위가 아닌 시간대는 쓰이지 않는다.
        assert_eq!(seconds % 900, 0, "{seconds}");
    }

    #[test]
    fn a_stamp_sorts_by_when_it_was_made() {
        let stamp = stamp();
        assert_eq!(stamp.len(), 15, "{stamp}");
        assert!(stamp.chars().filter(|c| *c == '-').count() == 1, "{stamp}");
    }
}
