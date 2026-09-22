//! 날짜 계산. 만료 임박을 판단하는 데만 쓴다.
//!
//! chrono 를 들이지 않는다. 필요한 건 "오늘로부터 며칠 남았나" 하나뿐이고,
//! 그 정도는 그레고리력 변환 두 개로 닫힌다. 시간대는 UTC 로 고정한다 —
//! 만료 임박을 하루 일찍 알리는 건 문제가 아니지만 의존성은 문제가 된다.

/// `YYYY-MM-DD` 한 줄. 저장 형식이자 표시 형식.
pub type Date = String;

/// days-from-epoch → (년, 월, 일). Howard Hinnant 의 civil_from_days.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// (년, 월, 일) → days-from-epoch. 위 함수의 역.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 오늘 (UTC).
pub fn today() -> Date {
    let (y, m, d) = civil_from_days(now_secs().div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// 지금 (UTC, ISO 8601). 검증 시각 기록에 쓴다.
pub fn now() -> String {
    let secs = now_secs();
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let t = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        t / 3600,
        (t % 3600) / 60,
        t % 60
    )
}

/// `YYYY-MM-DD` 를 days-from-epoch 로. 형식이 어긋나면 None.
pub fn parse(date: &str) -> Option<i64> {
    let mut parts = date.trim().split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // 2월 30일 같은 값을 거른다. 왕복시켜 같은 날짜가 나오는지 본다.
    let days = days_from_civil(y, m, d);
    (civil_from_days(days) == (y, m, d)).then_some(days)
}

/// 오늘부터 그 날짜까지 남은 일수. 지났으면 음수.
pub fn days_until(date: &str) -> Option<i64> {
    Some(parse(date)? - now_secs().div_euclid(86_400))
}

/// 오늘로부터 n 일 뒤. 폼 기본값에 쓴다.
pub fn plus_days(n: i64) -> Date {
    let (y, m, d) = civil_from_days(now_secs().div_euclid(86_400) + n);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_roundtrip_over_edge_cases() {
        for (y, m, d) in [
            (1970, 1, 1),
            (2000, 2, 29), // 400 으로 나뉘는 윤년
            (1900, 3, 1),  // 100 으로 나뉘지만 윤년이 아닌 해
            (2026, 9, 22),
            (2027, 12, 31),
        ] {
            assert_eq!(civil_from_days(days_from_civil(y, m, d)), (y, m, d));
        }
    }

    #[test]
    fn epoch_is_day_zero() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }

    #[test]
    fn rejects_impossible_dates() {
        assert!(parse("2026-02-30").is_none(), "2월 30일은 없다");
        assert!(parse("2025-02-29").is_none(), "2025 는 윤년이 아니다");
        assert!(parse("2026-13-01").is_none());
        assert!(parse("2026-9").is_none());
        assert!(parse("어제").is_none());
        assert!(parse("2026-02-28").is_some());
        assert!(parse("2028-02-29").is_some(), "2028 은 윤년이다");
    }

    #[test]
    fn days_until_counts_forward_and_backward() {
        assert_eq!(days_until(&today()), Some(0));
        assert_eq!(days_until(&plus_days(7)), Some(7));
        assert_eq!(days_until(&plus_days(-3)), Some(-3));
    }
}
