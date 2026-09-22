//! 마스터 계정을 붙일 때 사람이 넣는 값.
//!
//! provider 마다 자격의 구성이 다르다. 이름표 붙은 문자열 지도(`HashMap`)로 다루면
//! 오타 난 열쇠와 provider 에 맞지 않는 조합을 컴파일러가 잡아 주지 못한다.

pub mod secret;

use crate::account::Provider;
use self::secret::Secret;

/// provider 별 자격 구성.
pub enum CredentialInput {
    Github {
        token: Secret,
    },
    Aws {
        access_key_id: String,
        secret_access_key: Secret,
    },
    /// 브라우저 로그인으로만 붙는다. 사람이 적어 넣을 값이 없다.
    Browser,
}

impl CredentialInput {
    pub fn provider_matches(&self, provider: Provider) -> bool {
        matches!(
            (self, provider),
            (CredentialInput::Github { .. }, Provider::Github)
                | (CredentialInput::Aws { .. }, Provider::Aws)
                | (
                    CredentialInput::Browser,
                    Provider::Gcloud | Provider::Firebase
                )
        )
    }

    /// 비어 있는 칸을 사람이 읽을 이름으로 알려 준다.
    pub fn missing(&self) -> Option<&'static str> {
        match self {
            CredentialInput::Github { token } => token.is_empty().then_some("개인 액세스 토큰"),
            CredentialInput::Aws {
                access_key_id,
                secret_access_key,
            } => {
                if access_key_id.trim().is_empty() {
                    Some("액세스 키 ID")
                } else if secret_access_key.is_empty() {
                    Some("시크릿 액세스 키")
                } else {
                    None
                }
            }
            CredentialInput::Browser => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_credential_belongs_to_one_provider() {
        let github = CredentialInput::Github {
            token: Secret::new("t"),
        };
        assert!(github.provider_matches(Provider::Github));
        assert!(!github.provider_matches(Provider::Aws));
        assert!(CredentialInput::Browser.provider_matches(Provider::Firebase));
        assert!(!CredentialInput::Browser.provider_matches(Provider::Github));
    }

    #[test]
    fn the_missing_field_is_named_the_way_the_form_names_it() {
        assert_eq!(
            CredentialInput::Github {
                token: Secret::new(" ")
            }
            .missing(),
            Some("개인 액세스 토큰")
        );
        assert_eq!(
            CredentialInput::Aws {
                access_key_id: "AKIA".into(),
                secret_access_key: Secret::new(""),
            }
            .missing(),
            Some("시크릿 액세스 키")
        );
        assert_eq!(CredentialInput::Browser.missing(), None);
    }
}
