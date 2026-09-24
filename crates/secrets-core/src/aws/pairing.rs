//! 손에 든 pem 이 그 리전의 어느 키페어인지 가린다.
//!
//! 파일 이름은 키페어 이름의 단서일 뿐이다. 사람들이 받은 pem 의 이름을 바꾸고
//! (`LightsailDefaultKey-ap-northeast-2.pem` → `lightsail-seoul.pem`), 그러면 이름으로는
//! 찾을 수 없다. 지문이 맞는 키페어가 있으면 이름이 무엇이든 그것이다.

use super::{AwsError, fingerprint};

/// AWS 가 보는 키페어 하나.
#[derive(Debug, Clone)]
pub struct SeenKeyPair {
    pub name: String,
    /// AWS 가 적은 그대로. 주지 않으면 `None`.
    pub fingerprint: Option<String>,
}

/// 가려낸 키페어.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pairing {
    /// 지문이 맞았다. 이름은 AWS 의 것이다.
    Verified(String),
    /// 그 이름의 키페어는 있지만 AWS 가 지문을 주지 않아 맞춰 보지 못했다.
    Unverified(String),
}

impl Pairing {
    pub fn name(&self) -> &str {
        match self {
            Pairing::Verified(name) | Pairing::Unverified(name) => name,
        }
    }
}

pub struct Pairer;

impl Pairer {
    /// `mine` 은 손에 든 키의 지문을 여러 방식으로 적은 것이다.
    ///
    /// 지문이 맞는 키페어가 이름보다 앞선다. 지문이 맞는 것이 없으면 이름으로 찾되,
    /// AWS 가 지문을 주었는데 다르면 다른 키다.
    pub fn pair(named: &str, mine: &[String], seen: &[SeenKeyPair]) -> Result<Pairing, AwsError> {
        let matched = seen.iter().find(|pair| {
            pair.fingerprint
                .as_deref()
                .is_some_and(|theirs| fingerprint::any_same(mine, theirs))
        });
        if let Some(pair) = matched {
            return Ok(Pairing::Verified(pair.name.clone()));
        }

        let Some(pair) = seen.iter().find(|pair| pair.name == named) else {
            return Err(AwsError::Absent {
                name: named.to_string(),
                present: seen.iter().map(|pair| pair.name.clone()).collect(),
            });
        };
        match &pair.fingerprint {
            Some(expected) => Err(AwsError::Mismatch {
                expected: expected.clone(),
                found: mine.first().cloned().unwrap_or_default(),
            }),
            None => Ok(Pairing::Unverified(pair.name.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SSH: &str = "SHA256:gzsyNYxNoWmZrAdAQCtVPqaIoTmXn6cHnHfE4XSZjsM";
    const MD5: &str = "40:85:97:55:41:7e:25:a3:b0:f5:09:8e:3c:33:e9:c2";
    const OTHER_MD5: &str = "1f:51:ae:28:bf:89:e9:d8:1f:25:5d:37:2d:7d:b8:ca";

    fn mine() -> Vec<String> {
        vec![SSH.to_string(), MD5.to_string()]
    }

    fn seen(name: &str, fingerprint: Option<&str>) -> SeenKeyPair {
        SeenKeyPair {
            name: name.to_string(),
            fingerprint: fingerprint.map(str::to_string),
        }
    }

    #[test]
    fn a_renamed_file_is_paired_with_the_key_pair_its_fingerprint_matches() {
        let region = [seen("tuk-key", Some(OTHER_MD5)), seen("LightsailDefaultKeyPair", Some(MD5))];
        let paired = Pairer::pair("lightsail-seoul", &mine(), &region).unwrap();
        assert_eq!(paired, Pairing::Verified("LightsailDefaultKeyPair".into()));
    }

    #[test]
    fn the_fingerprint_wins_over_a_name_that_belongs_to_another_key_pair() {
        let region = [seen("tuk-key", Some(OTHER_MD5)), seen("real", Some(MD5))];
        let paired = Pairer::pair("tuk-key", &mine(), &region).unwrap();
        assert_eq!(paired.name(), "real");
    }

    #[test]
    fn a_named_key_pair_with_a_different_fingerprint_is_a_mismatch() {
        let region = [seen("tuk-key", Some(OTHER_MD5))];
        let refused = Pairer::pair("tuk-key", &mine(), &region).unwrap_err();
        assert!(matches!(refused, AwsError::Mismatch { .. }), "{refused}");
    }

    #[test]
    fn a_named_key_pair_without_a_fingerprint_is_taken_unverified() {
        let region = [seen("tuk-key", None)];
        let paired = Pairer::pair("tuk-key", &mine(), &region).unwrap();
        assert_eq!(paired, Pairing::Unverified("tuk-key".into()));
    }

    #[test]
    fn no_name_and_no_fingerprint_match_names_what_the_region_has() {
        let region = [seen("tuk-key", Some(OTHER_MD5)), seen("vpn", None)];
        let refused = Pairer::pair("lightsail-seoul", &mine(), &region).unwrap_err();
        let AwsError::Absent { present, .. } = &refused else {
            panic!("{refused}");
        };
        assert_eq!(present, &["tuk-key", "vpn"]);
    }

    #[test]
    fn an_empty_region_is_absent() {
        let refused = Pairer::pair("tuk-key", &mine(), &[]).unwrap_err();
        assert!(matches!(refused, AwsError::Absent { .. }));
    }
}
