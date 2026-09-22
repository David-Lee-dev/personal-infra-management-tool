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
        "다른 계정의 자격입니다. 이 계정은 {expected} 인데 넣은 자격은 {actual} 입니다"
    ))
}

