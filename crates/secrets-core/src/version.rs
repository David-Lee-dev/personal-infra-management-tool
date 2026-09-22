//! CLI 가 뱉는 버전 문자열에서 숫자를 뽑아내고 비교한다.
//!
//! 툴마다 출력 형식이 제각각이다 — `gh version 2.85.0 (2026-01-14)`,
//! `aws-cli/2.34.64 Python/3.14.5`, `OpenSSH_9.9p2`. 공통점은 처음 등장하는
//! `숫자.숫자[.숫자…]` 가 우리가 원하는 버전이라는 것뿐이라, 그 규칙만 쓴다.
//! 정규식 크레이트를 들이지 않으려는 목적도 있다.

use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, Clone)]
pub struct Version(Vec<u64>);

/// 동등성은 Ord 와 같은 규칙을 따라야 한다. 파생 구현을 쓰면 `2.85` 와 `2.85.0` 이
/// 다른 값이 되어 비교 결과와 어긋난다.
impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

impl Version {
    /// `"2.85.0"` 처럼 이미 잘린 문자열을 파싱한다.
    pub fn parse(text: &str) -> Option<Version> {
        let parts: Vec<u64> = text
            .split('.')
            .map(|p| p.trim().parse::<u64>().ok())
            .collect::<Option<_>>()?;
        (!parts.is_empty()).then_some(Version(parts))
    }

    /// CLI 출력 전체에서 첫 번째 버전처럼 보이는 토막을 찾는다.
    pub fn from_output(output: &str) -> Option<Version> {
        let bytes = output.as_bytes();
        let mut i = 0;

        while i < bytes.len() {
            if !bytes[i].is_ascii_digit() {
                i += 1;
                continue;
            }

            let start = i;
            let mut dots = 0;
            // 숫자와 점만 이어 붙인다. `9.9p2` 의 `p2` 같은 꼬리는 여기서 끊긴다.
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                if bytes[i] == b'.' {
                    // `1.2.` 처럼 점으로 끝나면 그 점은 버전의 일부가 아니다.
                    if i + 1 >= bytes.len() || !bytes[i + 1].is_ascii_digit() {
                        break;
                    }
                    dots += 1;
                }
                i += 1;
            }

            // 점이 하나도 없는 건 날짜나 연도일 가능성이 커서 버린다.
            if dots >= 1
                && let Some(v) = Version::parse(&output[start..i])
            {
                return Some(v);
            }

            // 숫자 구간을 건너뛴다. 무한 루프 방지.
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
        }
        None
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    /// 자리수가 다르면 없는 자리를 0 으로 본다. `2.85` 와 `2.85.0` 은 같다.
    fn cmp(&self, other: &Self) -> Ordering {
        let len = self.0.len().max(other.0.len());
        for i in 0..len {
            let a = self.0.get(i).copied().unwrap_or(0);
            let b = other.0.get(i).copied().unwrap_or(0);
            match a.cmp(&b) {
                Ordering::Equal => continue,
                other => return other,
            }
        }
        Ordering::Equal
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text: Vec<String> = self.0.iter().map(|n| n.to_string()).collect();
        write!(f, "{}", text.join("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn extracts_from_real_cli_output() {
        let cases = [
            ("gh version 2.85.0 (2026-01-14)", "2.85.0"),
            ("aws-cli/2.34.64 Python/3.14.5 Darwin/25.2.0", "2.34.64"),
            ("git version 2.39.5 (Apple Git-154)", "2.39.5"),
            ("OpenSSH_9.9p2, LibreSSL 3.3.6", "9.9"),
            ("Google Cloud SDK 509.0.0", "509.0.0"),
            ("14.7.0", "14.7.0"),
            ("age version v1.2.1", "1.2.1"),
        ];
        for (output, expected) in cases {
            assert_eq!(
                Version::from_output(output).unwrap(),
                v(expected),
                "출력: {output}"
            );
        }
    }

    #[test]
    fn ignores_bare_integers() {
        assert_eq!(Version::from_output("build 2026 nightly"), None);
    }

    #[test]
    fn no_version_at_all() {
        assert_eq!(Version::from_output("command not found"), None);
    }

    #[test]
    fn compares_across_lengths() {
        assert_eq!(v("2.85"), v("2.85.0"));
        assert!(v("2.85.0") > v("2.40"));
        assert!(v("2.9.0") < v("2.10.0"));
        assert!(v("1.0.0") > v("0.99.99"));
    }
}
