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

/// `from` 에서 `to` 까지 남은 일수. 지났으면 음수. 어느 쪽이든 못 읽으면 None.
///
/// 지금이 언제인지는 [`crate::port::Clock`] 이 말해 준다. 이 모듈은 날짜 계산만 한다.
pub fn days_between(from: &str, to: &str) -> Option<i64> {
    Some(parse(to)? - parse(from)?)
}

/// 그 날짜로부터 n 일 뒤.
pub fn plus_days(from: &str, n: i64) -> Option<Date> {
    let (y, m, d) = civil_from_days(parse(from)? + n);
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

/// 유닉스 초를 날짜와 시각으로. 시계 구현이 쓴다.
pub fn from_unix_seconds(secs: i64) -> (Date, String) {
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let t = secs.rem_euclid(86_400);
    (
        format!("{y:04}-{m:02}-{d:02}"),
        format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
            t / 3600,
            (t % 3600) / 60,
            t % 60
        ),
    )
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
    fn days_between_counts_forward_and_backward() {
        assert_eq!(days_between("2026-09-22", "2026-09-22"), Some(0));
        assert_eq!(days_between("2026-09-22", "2026-09-29"), Some(7));
        assert_eq!(days_between("2026-09-22", "2026-09-19"), Some(-3));
        assert_eq!(days_between("2026-09-22", "언젠가"), None);
    }

    #[test]
    fn plus_days_crosses_month_and_year_ends() {
        assert_eq!(plus_days("2026-09-22", 10).as_deref(), Some("2026-10-02"));
        assert_eq!(plus_days("2026-12-31", 1).as_deref(), Some("2027-01-01"));
        assert_eq!(plus_days("2028-02-28", 1).as_deref(), Some("2028-02-29"));
    }

    #[test]
    fn unix_seconds_become_a_date_and_a_timestamp() {
        let (day, moment) = from_unix_seconds(1_774_000_000);
        assert_eq!(day, "2026-03-20");
        assert!(moment.starts_with("2026-03-20T"), "{moment}");
        assert!(moment.ends_with('Z'));
    }
}
