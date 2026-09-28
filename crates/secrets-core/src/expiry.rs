//! 자격의 만료일과, 그 날짜가 오늘에 얼마나 가까운가.
//!
//! 계정의 자격이든 금고의 키든 같은 규칙이다. 만료일은 `YYYY-MM-DD`, 기한이 없음을
//! 확인했으면 [`NEVER`], 모르면 비워 둔다. 오늘은 호출자가 넘긴다 — 판정이 실제 시각에
//! 묶이지 않게.

use serde::{Deserialize, Serialize};

use crate::time;

/// 만료가 이만큼 남으면 상시로 알린다.
pub const WARN_WITHIN_DAYS: i64 = 7;

/// 만료일 자리에 이 값이 적히면 기한이 없는 자격이라는 뜻이다.
pub const NEVER: &str = "never";

/// 자격이 만료에 얼마나 가까운가.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Expiry {
    /// 만료일이 적혀 있지 않다. 아직 확인하지 않았다는 뜻이다.
    Unset,
    /// 기한이 없는 자격임을 확인했다.
    ///
    /// 알릴 일은 없지만 좋은 상태도 아니다 — 무기한 자격은 유출돼도 스스로
    /// 만료되지 않으므로, 확인됐다는 사실만 기록하고 화면에서 구분해 보여준다.
    Never,
    Ok,
    /// 기한이 임박했다. 남은 일수를 들고 있다.
    Soon(i64),
    /// 이미 지났다. 지난 일수.
    Expired(i64),
}

impl Expiry {
    /// 적힌 만료일을 오늘 기준으로 판정한다. 읽을 수 없는 값은 적히지 않은 것으로 본다.
    pub fn of(expires: Option<&str>, today: &str) -> Expiry {
        let Some(raw) = expires.map(str::trim) else {
            return Expiry::Unset;
        };
        if raw == NEVER {
            return Expiry::Never;
        }
        let Some(days) = time::days_between(today, raw) else {
            return Expiry::Unset;
        };
        if days < 0 {
            Expiry::Expired(-days)
        } else if days <= WARN_WITHIN_DAYS {
            Expiry::Soon(days)
        } else {
            Expiry::Ok
        }
    }

    /// 지금 사람에게 알려야 하는 상태인가.
    pub fn needs_attention(self) -> bool {
        matches!(self, Expiry::Soon(_) | Expiry::Expired(_))
    }
}

/// 사람이 적은 만료일. 빈 값은 "모름"으로 지운다.
pub fn check_expires(text: &str) -> Result<Option<String>, String> {
    let value = text.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value == NEVER {
        return Ok(Some(NEVER.to_string()));
    }
    let shaped = value.len() == 10 && value.as_bytes()[4] == b'-' && value.as_bytes()[7] == b'-';
    if !shaped || time::parse(value).is_none() {
        return Err(format!(
            "만료일 {value}을(를) 읽지 못했습니다. YYYY-MM-DD로 적거나, 기한이 없으면 '기한 없음'을 고르세요."
        ));
    }
    Ok(Some(value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODAY: &str = "2026-09-28";

    #[test]
    fn judges_the_date_against_today() {
        assert_eq!(Expiry::of(None, TODAY), Expiry::Unset);
        assert_eq!(Expiry::of(Some("never"), TODAY), Expiry::Never);
        assert_eq!(Expiry::of(Some("2026-10-01"), TODAY), Expiry::Soon(3));
        assert_eq!(Expiry::of(Some("2027-01-01"), TODAY), Expiry::Ok);
        assert_eq!(Expiry::of(Some("2026-09-26"), TODAY), Expiry::Expired(2));
        assert_eq!(Expiry::of(Some("언젠가"), TODAY), Expiry::Unset);
        assert!(Expiry::Expired(1).needs_attention() && !Expiry::Never.needs_attention());
    }

    #[test]
    fn a_written_date_must_be_a_real_day_or_never_or_blank() {
        assert_eq!(check_expires("  2027-02-28 ").unwrap().as_deref(), Some("2027-02-28"));
        assert_eq!(check_expires("never").unwrap().as_deref(), Some(NEVER));
        assert_eq!(check_expires("   ").unwrap(), None);
        for bad in ["2027-02-30", "27-1-1", "내년", "2027/01/01"] {
            assert!(check_expires(bad).is_err(), "{bad}");
        }
    }
}
