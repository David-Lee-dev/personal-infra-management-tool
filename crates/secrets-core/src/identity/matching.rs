//! 두 자격이 같은 계정의 것인가.

/// 새 자격이 같은 계정의 것인가.
///
/// 아니라면 이름만 같고 속은 다른 계정이 되고, 나중에 알아챌 방법이 없다.
/// 아직 확인한 적 없는 계정은 비교할 대상이 없으므로 통과시킨다.
pub fn same_account(expected: &str, actual: &str) -> Result<(), String> {
    if expected.is_empty() || expected == actual {
        return Ok(());
    }
    Err(format!(
        "다른 계정의 자격 증명입니다. 이 계정의 신원은 {expected}이며, 입력한 자격 증명의 신원은 {actual}입니다."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_credential_from_another_account_is_refused() {
        assert!(same_account("octocat", "octocat").is_ok());
        assert!(same_account("", "누구든").is_ok(), "확인한 적 없으면 비교하지 않는다");
        // 대소문자가 다르면 다른 계정이다. GitHub 로그인은 대소문자를 보존한다.
        assert!(same_account("David-Lee-dev", "david-lee-dev").is_err());
    }

}
