//! 관찰한 신원에서 계정 이름을 짓는 규칙.
//!
//! 밖에서 읽은 문자열을 **해석하는** 일은 어댑터가 하고, 그 값으로 무엇을 **이름
//! 삼을지** 정하는 일은 여기서 한다. 어댑터가 슬러그까지 만들어 돌려주면 이 규칙이
//! provider 수만큼 갈라진다.

/// 이름을 슬러그로 바꾼다. 영숫자가 아닌 것은 하이픈 하나로 접는다.
pub(super) fn slugify(text: &str) -> String {
    let mut slug = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.extend(ch.to_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').chars().take(48).collect::<String>()
}

#[cfg(test)]
mod tests {
    
    use crate::identity::{AwsPrincipalKind, ObservedIdentity};
    #[test]
    fn aws_users_in_one_account_get_different_slugs() {
        let user = |name: &str| ObservedIdentity::Aws {
            arn: format!("arn:aws:iam::320042238085:user/{name}"),
            account_id: "320042238085".into(),
            principal_name: name.into(),
            principal_kind: AwsPrincipalKind::User,
            alias: None,
        };
        // 계정 번호로 이름을 지으면 둘이 충돌한다.
        assert_ne!(user("david-admin").slug(), user("tuk-dev-power").slug());
        assert_eq!(user("david-admin").slug(), "david-admin");
    }

    #[test]
    fn an_email_slug_stays_within_the_slug_rules() {
        let google = ObservedIdentity::Google {
            email: "Tuk.Kim+dev@tuk.im".into(),
            project: None,
        };
        let slug = google.slug();
        assert_eq!(slug, "tuk-kim-dev-tuk-im");
        assert!(crate::account::validate_slug(&slug).is_ok(), "{slug}");
    }

}
