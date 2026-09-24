//! 같은 키인지 지문으로 맞춰 본다.
//!
//! AWS 와 `ssh-keygen` 이 같은 값을 다르게 적는다. 이 차이를 모르면 손에 든 pem 이
//! 그 키페어의 것인지 확인할 수 없고, 그러면 엉뚱한 키를 금고에 들이게 된다.
//!
//! ```text
//! ssh-keygen  SHA256:gzsyNYxNoWmZrAdAQCtVPqaIoTmXn6cHnHfE4XSZjsM
//! AWS                gzsyNYxNoWmZrAdAQCtVPqaIoTmXn6cHnHfE4XSZjsM=
//! ```
//!
//! RSA 키페어에는 AWS 가 SHA256 이 아니라 콜론으로 이은 다이제스트를 준다 — 가져온
//! 키와 Lightsail 은 공개 키 DER 의 MD5, EC2 가 만든 키는 PKCS#8 DER 의 SHA1.
//! `ssh-keygen` 은 이 값을 내지 않으므로 어댑터가 같은 방식으로 계산해 넘긴다.
//! 여기서는 적는 방식만 맞춘다.

/// SHA256 지문을 비교할 수 있는 형태로 고른다. SHA256 이 아니면 `None`.
pub fn normalize(text: &str) -> Option<String> {
    let body = text.trim().strip_prefix("SHA256:").unwrap_or(text.trim());
    let body = body.trim_end_matches('=');

    // base64 로 적은 SHA-256 은 패딩을 빼면 43 글자다. 콜론이 섞인 MD5·SHA1 표기는
    // 길이도 글자도 달라서 여기서 걸러진다.
    if body.len() != 43 || !body.bytes().all(is_base64) {
        return None;
    }
    Some(body.to_string())
}

fn is_base64(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/'
}

/// 콜론으로 이은 16진 다이제스트를 소문자로 고른다. MD5(16바이트)·SHA1(20바이트)만.
fn digest(text: &str) -> Option<String> {
    let body = text.trim();
    let body = body.strip_prefix("MD5:").unwrap_or(body);
    let pairs: Vec<&str> = body.split(':').collect();
    let sized = pairs.len() == 16 || pairs.len() == 20;
    let hex = pairs
        .iter()
        .all(|pair| pair.len() == 2 && pair.bytes().all(|b| b.is_ascii_hexdigit()));
    (sized && hex).then(|| body.to_ascii_lowercase())
}

/// 두 지문이 같은 키를 가리키는가. 적는 방식이 서로 다른 지문은 같지 않다.
pub fn same(one: &str, other: &str) -> bool {
    if let (Some(a), Some(b)) = (normalize(one), normalize(other)) {
        return a == b;
    }
    match (digest(one), digest(other)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// 한 키를 여러 방식으로 적은 지문 중 하나라도 AWS 의 것과 같은가.
pub fn any_same(mine: &[String], theirs: &str) -> bool {
    mine.iter().any(|one| same(one, theirs))
}

#[cfg(test)]
mod tests {
    use super::*;

    const AWS: &str = "gzsyNYxNoWmZrAdAQCtVPqaIoTmXn6cHnHfE4XSZjsM=";
    const SSH: &str = "SHA256:gzsyNYxNoWmZrAdAQCtVPqaIoTmXn6cHnHfE4XSZjsM";

    #[test]
    fn the_same_key_written_two_ways_is_recognised() {
        assert!(same(AWS, SSH), "AWS 와 ssh-keygen 은 같은 값을 다르게 적는다");
        assert!(same(SSH, AWS));
    }

    #[test]
    fn different_keys_do_not_match() {
        let other = "DZx5EZ24GNKWDomtlQvsStpBOfsiROnOx0ypcIkOsTs=";
        assert!(!same(AWS, other));
    }

    const MD5: &str = "40:85:97:55:41:7e:25:a3:b0:f5:09:8e:3c:33:e9:c2";

    #[test]
    fn a_colon_digest_matches_itself_regardless_of_case_and_prefix() {
        assert!(same(MD5, &MD5.to_ascii_uppercase()));
        assert!(same(&format!("MD5:{MD5}"), MD5));
        let sha1 = "45:62:36:7a:38:b4:fa:c9:f6:1b:eb:05:4c:d2:ad:2b:d8:5d:87:31";
        assert!(same(sha1, sha1));
    }

    #[test]
    fn a_colon_digest_differs_from_another_digest_and_from_sha256() {
        let other = "1f:51:ae:28:bf:89:e9:d8:1f:25:5d:37:2d:7d:b8:ca";
        assert!(!same(MD5, other));
        assert!(!same(MD5, AWS), "적는 방식이 다르면 같은 키인지 알 수 없다");
    }

    #[test]
    fn any_same_finds_the_one_written_the_way_aws_writes_it() {
        let mine = vec![SSH.to_string(), MD5.to_string()];
        assert!(any_same(&mine, MD5));
        assert!(any_same(&mine, AWS));
        assert!(!any_same(&mine, "1f:51:ae:28:bf:89:e9:d8:1f:25:5d:37:2d:7d:b8:ca"));
        assert!(!any_same(&[], MD5));
    }

    #[test]
    fn junk_is_not_a_fingerprint() {
        for text in ["", "SHA256:", "그냥 글자", "SHA256:짧음", "40:85:97", "zz:85:97:55:41:7e:25:a3:b0:f5:09:8e:3c:33:e9:c2"] {
            assert_eq!(normalize(text), None, "{text}");
            assert!(!same(text, text), "{text}");
        }
    }
}
