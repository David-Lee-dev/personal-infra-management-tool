//! 계정 자격의 만료 — 판정 규칙은 [`crate::expiry`] 가 가진다.

use crate::expiry::Expiry;

use super::Account;

impl Account {
    /// 만료까지 얼마나 남았는가. 오늘이 언제인지는 호출자가 안다.
    pub fn expiry_on(&self, today: &str) -> Expiry {
        Expiry::of(self.expires.as_deref(), today)
    }

    /// 지금 사람에게 알려야 하는 상태인가.
    pub fn needs_attention_on(&self, today: &str) -> bool {
        self.expiry_on(today).needs_attention()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{Account, NEVER, Provider};
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
