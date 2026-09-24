//! IAM 이름 규칙 — `<앱>-<환경>-<권한>-iam-<YYYYMMDD>`.
//!
//! 이름만 보고 누가 · 어디서 · 무엇을 하는지, 그리고 언제 만든 세대인지 알 수 있어야
//! 한다. 키는 바꾸지 않고 새로 만들어 옮기므로, 같은 권한의 옛 것과 새 것이 한동안
//! 나란히 있다. 날짜가 둘을 가른다. 사람이 치지 않고 여기서만 만든다.

use crate::aws::iam::policy::Policy;

/// AWS 가 받는 IAM 사용자 이름의 최대 길이.
pub const MAX_LEN: usize = 64;
const SUFFIX: &str = "iam";

/// 어느 환경의 IAM 인가. 환경끼리 IAM 을 나눠 쓰지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Env {
    Prod,
    Dev,
    /// 이 맥의 개발용. dev 서버와 IAM 을 나누면 맥에서 샌 키가 서버에 닿지 않는다.
    Local,
}

impl Env {
    pub fn id(&self) -> &'static str {
        match self {
            Env::Prod => "prod",
            Env::Dev => "dev",
            Env::Local => "local",
        }
    }

    pub fn parse(text: &str) -> Option<Env> {
        match text.trim() {
            "prod" => Some(Env::Prod),
            "dev" => Some(Env::Dev),
            "local" => Some(Env::Local),
            _ => None,
        }
    }
}

/// 이름을 이루는 조각.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IamName {
    pub app: String,
    pub env: Env,
    pub perm: String,
    /// 만든 날 `YYYYMMDD`. 같은 날 같은 이름이 또 나오면 `-2` 부터 붙는다.
    pub stamp: String,
}

impl IamName {
    /// 조각들로 이름을 만든다. `today` 는 `YYYY-MM-DD`. 조각이 규칙에 맞지 않거나
    /// 너무 길면 `None`.
    pub fn compose(app: &str, env: Env, perm: &str, today: &str) -> Option<IamName> {
        let stamp: String = today.chars().filter(char::is_ascii_digit).collect();
        let name = IamName {
            app: app.trim().to_string(),
            env,
            perm: perm.trim().to_string(),
            stamp,
        };
        let valid = Naming::is_piece(&name.app)
            && Naming::is_piece(&name.perm)
            && name.stamp.len() == 8
            && name.full().len() <= MAX_LEN;
        valid.then_some(name)
    }

    /// 같은 날 같은 이름이 이미 있을 때 뒤에 붙일 번호. 2 부터.
    pub fn numbered(&self, n: u32) -> Option<IamName> {
        let base: String = self.stamp.chars().take(8).collect();
        let next = IamName {
            stamp: format!("{base}-{n}"),
            ..self.clone()
        };
        (next.full().len() <= MAX_LEN).then_some(next)
    }

    pub fn full(&self) -> String {
        format!("{}-{}-{}-{SUFFIX}-{}", self.app, self.env.id(), self.perm, self.stamp)
    }
}

/// 같은 앱 · 같은 환경에 이미 있는 IAM 하나. 권한 조각을 정할 때 견준다.
#[derive(Debug, Clone)]
pub struct Sibling {
    pub perm: String,
    /// 그 IAM 의 허용 대상.
    pub resources: Vec<String>,
}

pub struct Naming;

impl Naming {
    /// 이름 조각으로 쓸 수 있는가 — 소문자 · 숫자 · `-`, 앞뒤는 `-` 가 아니다.
    pub fn is_piece(text: &str) -> bool {
        !text.is_empty()
            && !text.starts_with('-')
            && !text.ends_with('-')
            && text
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    }

    /// 아무 문자열을 이름 조각으로 줄인다. 남는 것이 없으면 빈 문자열.
    pub fn slug(text: &str) -> String {
        let mut out = String::new();
        for c in text.trim().to_ascii_lowercase().chars() {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                out.push(c);
            } else if !out.ends_with('-') {
                out.push('-');
            }
        }
        out.trim_matches('-').to_string()
    }

    /// 정책에서 권한 조각을 정한다.
    ///
    /// 서비스가 하나면 그 이름이다. 같은 앱 · 환경에 그 조각을 쓰는 IAM 이 이미 있으면
    /// 둘을 견준다.
    /// - 대상이 같으면 **같은 권한의 새 세대**다. 조각은 그대로 두고 날짜가 가른다.
    /// - 대상이 다르면 **다른 권한**이다. 대상의 마지막 경로 조각을 붙인다
    ///   (`s3-applog-archive`). 그것도 다른 대상에 쓰였거나 뽑을 수 없으면 정하지
    ///   못한다 — 그때는 사람이 친다.
    pub fn perm_for(policy: &Policy, siblings: &[Sibling]) -> Option<String> {
        let services = policy.services();
        let [service] = services.as_slice() else {
            return None;
        };
        let mine = Naming::resource_set(policy.resources().into_iter().map(str::to_string));
        let fits = |perm: &str| {
            siblings
                .iter()
                .filter(|s| s.perm == perm)
                .all(|s| Naming::resource_set(s.resources.iter().cloned()) == mine)
        };

        let base = Naming::slug(service);
        if fits(&base) {
            return Some(base);
        }
        let longer = format!("{base}-{}", Naming::shared_segment(policy)?);
        fits(&longer).then_some(longer)
    }

    fn resource_set(resources: impl Iterator<Item = String>) -> Vec<String> {
        let mut set: Vec<String> = resources.collect();
        set.sort();
        set.dedup();
        set
    }

    /// 모든 대상이 가리키는 마지막 경로 조각. 하나로 모이지 않으면 `None`.
    ///
    /// 버킷 자체처럼 경로가 없는 대상은 건너뛴다. 버킷과 그 안의 경로를 함께 주는
    /// 정책이 흔한데, 구분되는 것은 경로 쪽이다.
    fn shared_segment(policy: &Policy) -> Option<String> {
        let mut found: Option<String> = None;
        for resource in policy.resources() {
            let Some((_, path)) = resource.split_once('/') else {
                continue;
            };
            let Some(last) = path
                .split('/')
                .rev()
                .map(|part| part.trim_matches('*'))
                .find(|part| !part.is_empty())
            else {
                continue;
            };
            let slug = Naming::slug(last);
            if slug.is_empty() {
                return None;
            }
            match &found {
                Some(seen) if *seen != slug => return None,
                _ => found = Some(slug),
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODAY: &str = "2026-09-24";

    fn policy(resources: &[&str], actions: &[&str]) -> Policy {
        let doc = serde_json::json!({
            "Version": "2012-10-17",
            "Statement": [{ "Effect": "Allow", "Action": actions, "Resource": resources }]
        });
        Policy::read(&doc.to_string()).unwrap()
    }

    fn sibling(perm: &str, resources: &[&str]) -> Sibling {
        Sibling {
            perm: perm.into(),
            resources: resources.iter().map(|r| r.to_string()).collect(),
        }
    }

    mod compose {
        use super::*;

        #[test]
        fn joins_app_env_perm_suffix_and_the_day_it_was_made() {
            let name = IamName::compose("tuk-api", Env::Prod, "s3", TODAY).unwrap();
            assert_eq!(name.full(), "tuk-api-prod-s3-iam-20260924");
        }

        #[test]
        fn a_second_one_on_the_same_day_is_numbered() {
            let name = IamName::compose("tuk-api", Env::Prod, "s3", TODAY).unwrap();
            assert_eq!(name.numbered(2).unwrap().full(), "tuk-api-prod-s3-iam-20260924-2");
        }

        #[test]
        fn refuses_uppercase_spaces_and_edge_dashes() {
            assert!(IamName::compose("Tuk", Env::Dev, "s3", TODAY).is_none());
            assert!(IamName::compose("tuk api", Env::Dev, "s3", TODAY).is_none());
            assert!(IamName::compose("-tuk", Env::Dev, "s3", TODAY).is_none());
            assert!(IamName::compose("tuk", Env::Dev, "", TODAY).is_none());
        }

        #[test]
        fn refuses_names_longer_than_aws_allows() {
            let app = "a".repeat(50);
            assert!(IamName::compose(&app, Env::Prod, "s3", TODAY).is_none());
        }

        #[test]
        fn refuses_a_day_that_is_not_a_date() {
            assert!(IamName::compose("tuk", Env::Dev, "s3", "어제").is_none());
        }
    }

    mod slug {
        use super::*;

        #[test]
        fn lowers_and_collapses_everything_else_into_single_dashes() {
            assert_eq!(Naming::slug("  Applog_Archive.v2 "), "applog-archive-v2");
            assert_eq!(Naming::slug("***"), "");
        }
    }

    mod perm_for {
        use super::*;

        #[test]
        fn a_single_service_names_the_permission() {
            let read = policy(&["arn:aws:s3:::bucket/*"], &["s3:PutObject"]);
            assert_eq!(Naming::perm_for(&read, &[]).as_deref(), Some("s3"));
        }

        #[test]
        fn the_same_resources_again_are_a_new_generation_of_the_same_permission() {
            let read = policy(&["arn:aws:s3:::bucket/*"], &["s3:PutObject", "s3:GetObject"]);
            let old = [sibling("s3", &["arn:aws:s3:::bucket/*"])];
            assert_eq!(Naming::perm_for(&read, &old).as_deref(), Some("s3"));
        }

        #[test]
        fn different_resources_on_the_same_service_get_the_last_resource_segment() {
            let read = policy(
                &["arn:aws:s3:::logs", "arn:aws:s3:::logs/applog-archive/*"],
                &["s3:PutObject"],
            );
            let other = [sibling("s3", &["arn:aws:s3:::public/avatars/*"])];
            assert_eq!(Naming::perm_for(&read, &other).as_deref(), Some("s3-applog-archive"));
        }

        #[test]
        fn resources_pointing_at_different_places_cannot_be_named() {
            let read = policy(
                &[
                    "arn:aws:bedrock:*::foundation-model/anthropic.claude-haiku",
                    "arn:aws:bedrock:*::foundation-model/amazon.nova-lite",
                ],
                &["bedrock:InvokeModel"],
            );
            let other = [sibling("bedrock", &["arn:aws:bedrock:*::foundation-model/x"])];
            assert_eq!(Naming::perm_for(&read, &other), None);
        }

        #[test]
        fn two_services_cannot_be_named() {
            let read = policy(&["arn:aws:s3:::b/*"], &["s3:PutObject", "bedrock:InvokeModel"]);
            assert_eq!(Naming::perm_for(&read, &[]), None);
        }

        #[test]
        fn a_whole_bucket_alone_has_no_segment_to_add() {
            let read = policy(&["arn:aws:s3:::bucket"], &["s3:ListBucket"]);
            let other = [sibling("s3", &["arn:aws:s3:::elsewhere/*"])];
            assert_eq!(Naming::perm_for(&read, &other), None);
        }
    }
}
