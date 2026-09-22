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

