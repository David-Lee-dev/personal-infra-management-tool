//! 자격이 만료에 얼마나 가까운가.
//!
//! 지금이 언제인지는 이 모듈이 알지 못한다. 오늘을 인자로 받아야 판정이 결정적이 되고,
//! 실제 시각에 묶이지 않는다.

use serde::{Deserialize, Serialize};

use crate::time;

use super::{Account, NEVER, WARN_WITHIN_DAYS};

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

impl Account {
    /// 만료까지 얼마나 남았는가. 오늘이 언제인지는 호출자가 안다.
    pub fn expiry_on(&self, today: &str) -> Expiry {
        let Some(raw) = self.expires.as_deref().map(str::trim) else {
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
    pub fn needs_attention_on(&self, today: &str) -> bool {
        matches!(self.expiry_on(today), Expiry::Soon(_) | Expiry::Expired(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{Account, Provider};
    use crate::time;

    const TODAY: &str = "2026-09-22";
    #[test]
    fn expiry_reads_the_date_the_way_the_list_shows_it() {
        let mut account = Account::new(Provider::Github, "octocat");
        assert!(matches!(account.expiry_on(TODAY), Expiry::Unset));

        account.expires = Some(NEVER.to_string());
        assert!(matches!(account.expiry_on(TODAY), Expiry::Never));

        account.expires = time::plus_days(TODAY, 3);
        assert!(matches!(account.expiry_on(TODAY), Expiry::Soon(3)), "곧 만료는 알려야 한다");
        assert!(account.needs_attention_on(TODAY));

        account.expires = time::plus_days(TODAY, 60);
        assert!(matches!(account.expiry_on(TODAY), Expiry::Ok));
        assert!(!account.needs_attention_on(TODAY));

        account.expires = time::plus_days(TODAY, -2);
        assert!(matches!(account.expiry_on(TODAY), Expiry::Expired(2)));
        assert!(account.needs_attention_on(TODAY));
    }

    #[test]
    fn what_to_do_when_it_expires_depends_on_the_credential() {
        assert!(
            Account::new(Provider::Github, "a")
                .renewal_hint()
                .contains("새 토큰을 발급")
        );
        assert!(
            Account::new(Provider::Gcloud, "a")
                .renewal_hint()
                .contains("다시 로그인")
        );
    }

}
