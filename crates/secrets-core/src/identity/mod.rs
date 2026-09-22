//! 관찰한 신원과, 그 신원에서 이름을 짓는 규칙.
//!
//! **밖에서 읽어 온 문자열을 해석하는 일은 어댑터**가 하고, **그 값으로 무엇을
//! 이름 삼을지 정하는 일은 여기서** 한다. 어댑터가 슬러그까지 만들어 돌려주면
//! 이름 규칙이 provider 수만큼 갈라져, 한 곳만 고치면 조용히 어긋난다.

mod matching;
mod naming;

pub use matching::same_account;

use self::naming::slugify;
use crate::account::Provider;

/// AWS 에서 자격이 가리키는 주체의 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwsPrincipalKind {
    User,
    AssumedRole,
}

/// CLI 출력에서 읽어 낸, 해석이 끝난 신원.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedIdentity {
    Github {
        login: String,
        user_id: String,
        /// 공개 이메일. 비공개면 비어서 온다.
        public_email: Option<String>,
    },
    Aws {
        arn: String,
        account_id: String,
        principal_name: String,
        principal_kind: AwsPrincipalKind,
        /// 계정 별칭. 읽을 권한이 없으면 없다.
        alias: Option<String>,
    },
    Google {
        email: String,
        /// 기본 프로젝트. gcloud 만 준다.
        project: Option<String>,
    },
}

/// 신원과 함께 읽히는 부수 사실.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountFacts {
    /// `YYYY-MM-DD` 또는 `never`. 알아내지 못했으면 없다.
    pub expires: Option<String>,
    pub scopes: Vec<String>,
    pub root_keys_present: Option<bool>,
    pub root_mfa: Option<bool>,
}

/// 준비 단계가 실제로 보고 온 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub identity: ObservedIdentity,
    pub facts: AccountFacts,
}

impl ObservedIdentity {
    pub fn provider(&self) -> Provider {
        match self {
            ObservedIdentity::Github { .. } => Provider::Github,
            ObservedIdentity::Aws { .. } => Provider::Aws,
            // gcloud 와 firebase 는 같은 Google 신원을 쓰지만 서로 다른 계정이다.
            // provider 는 신원이 아니라 준비를 시작한 쪽이 안다.
            ObservedIdentity::Google { project, .. } => {
                if project.is_some() {
                    Provider::Gcloud
                } else {
                    Provider::Firebase
                }
            }
        }
    }

    /// 이 계정에서 나를 가리키는 이름. 계정끼리 같은지 비교하는 기준이기도 하다.
    pub fn name(&self) -> &str {
        match self {
            ObservedIdentity::Github { login, .. } => login,
            ObservedIdentity::Aws { arn, .. } => arn,
            ObservedIdentity::Google { email, .. } => email,
        }
    }

    /// 자격의 종류. 목록에서 한눈에 구분하는 데 쓴다.
    pub fn kind(&self) -> &'static str {
        match self {
            ObservedIdentity::Github { .. } | ObservedIdentity::Google { .. } => "oauth",
            ObservedIdentity::Aws {
                principal_kind: AwsPrincipalKind::User,
                ..
            } => "iam-user",
            ObservedIdentity::Aws {
                principal_kind: AwsPrincipalKind::AssumedRole,
                ..
            } => "assumed-role",
        }
    }

    /// 디렉토리 이름이 될 슬러그.
    ///
    /// AWS 는 계정 번호나 별칭이 아니라 **IAM 주체 이름**에서 만든다. 한 AWS 계정에
    /// 사용자가 여럿이면 번호로는 서로 구분되지 않기 때문이다.
    pub fn slug(&self) -> String {
        match self {
            ObservedIdentity::Github { login, .. } => slugify(login),
            ObservedIdentity::Aws { principal_name, .. } => slugify(principal_name),
            ObservedIdentity::Google { email, .. } => slugify(email),
        }
    }

    /// 목록에 보여 줄 한 줄.
    pub fn display(&self) -> String {
        match self {
            ObservedIdentity::Github { login, .. } => login.clone(),
            ObservedIdentity::Aws {
                account_id, alias, ..
            } => match alias {
                Some(alias) => format!("{alias} ({account_id})"),
                None => account_id.clone(),
            },
            ObservedIdentity::Google { project, .. } => match project {
                Some(project) => format!("Google Cloud · {project}"),
                None => "Firebase".to_string(),
            },
        }
    }

    /// 이 계정으로 커밋할 때 쓸 이메일.
    ///
    /// GitHub 은 공개 이메일이 없으면 noreply 주소를 쓴다. 커밋이 계정에 붙으면서
    /// 실제 주소는 드러나지 않는다.
    pub fn git_email(&self) -> Option<String> {
        match self {
            ObservedIdentity::Github {
                login,
                user_id,
                public_email,
            } => match public_email.as_deref().map(str::trim).filter(|e| !e.is_empty()) {
                Some(email) => Some(email.to_string()),
                None => (!user_id.is_empty())
                    .then(|| format!("{user_id}+{login}@users.noreply.github.com")),
            },
            _ => None,
        }
    }

    /// AWS 계정 번호. 같은 계정에 속한 신원끼리 묶어 보기 위한 것이다.
    pub fn aws_account_id(&self) -> Option<String> {
        match self {
            ObservedIdentity::Aws { account_id, .. } => Some(account_id.clone()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn github(login: &str, id: &str, email: Option<&str>) -> ObservedIdentity {
        ObservedIdentity::Github {
            login: login.into(),
            user_id: id.into(),
            public_email: email.map(str::to_string),
        }
    }

    #[test]
    fn a_github_account_without_a_public_email_commits_as_noreply() {
        assert_eq!(
            github("octocat", "583231", None).git_email().as_deref(),
            Some("583231+octocat@users.noreply.github.com")
        );
        assert_eq!(
            github("octocat", "583231", Some("me@example.com"))
                .git_email()
                .as_deref(),
            Some("me@example.com")
        );
        // 비공개 이메일이 빈 문자열로 오는 경우도 같다.
        assert_eq!(
            github("octocat", "583231", Some("  ")).git_email().as_deref(),
            Some("583231+octocat@users.noreply.github.com")
        );
    }

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
    fn an_assumed_role_is_a_different_kind_than_a_user() {
        let role = ObservedIdentity::Aws {
            arn: "arn:aws:sts::1:assumed-role/admin/session".into(),
            account_id: "1".into(),
            principal_name: "admin".into(),
            principal_kind: AwsPrincipalKind::AssumedRole,
            alias: None,
        };
        assert_eq!(role.kind(), "assumed-role");
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

    #[test]
    fn an_alias_is_easier_to_read_than_the_account_number() {
        let with_alias = ObservedIdentity::Aws {
            arn: "arn:aws:iam::1:user/a".into(),
            account_id: "320042238085".into(),
            principal_name: "a".into(),
            principal_kind: AwsPrincipalKind::User,
            alias: Some("tuk".into()),
        };
        assert_eq!(with_alias.display(), "tuk (320042238085)");
    }

    #[test]
    fn a_credential_from_another_account_is_refused() {
        assert!(same_account("octocat", "octocat").is_ok());
        assert!(same_account("", "누구든").is_ok(), "확인한 적 없으면 비교하지 않는다");
        // 대소문자가 다르면 다른 계정이다. GitHub 로그인은 대소문자를 보존한다.
        assert!(same_account("David-Lee-dev", "david-lee-dev").is_err());
    }
}
