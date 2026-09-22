//! Google — gcloud 와 firebase. 둘 다 브라우저로 붙고 같은 Google 신원을 쓴다.
//!
//! gcloud 는 localhost 로 결과를 받아 스스로 끝내고, firebase 는 코드를 되돌려
//! 넣어야 끝난다. 그래서 firebase 만 두 단계다.

use std::io;

use secrets_core::account::Provider;
use secrets_core::identity::{AccountFacts, ObservedIdentity, Observation};

use super::{LoginFlow, Method, capture};

/// 브라우저로 붙는다. 받아 적을 값이 없다.
///
/// firebase 는 출력이 TTY 가 아니면 코드를 되돌려 넣는 흐름으로 빠지므로 두 단계다.
pub(super) fn method(provider: Provider) -> Method {
    Method {
        flow: if provider == Provider::Firebase {
            LoginFlow::BrowserCode
        } else {
            LoginFlow::BrowserCallback
        },
        fields: &[],
        browser: None,
        guidance: "브라우저가 열립니다. Google 계정으로 로그인하면 이 계정 전용 설정에만 기록되고, 지금 쓰고 있는 로그인은 그대로 남습니다.",
    }
}

/// gcloud 는 설정에서 계정과 기본 프로젝트를 읽는다.
pub(super) fn probe_gcloud(home_dir: &std::path::Path) -> io::Result<Observation> {
    let (outcome, raw) = capture(
        Provider::Gcloud,
        home_dir,
        "gcloud",
        &[
            "config",
            "list",
            "--format=value(core.account,core.project)",
        ],
    )?;

    let fields: Vec<&str> = raw.trim().split('\t').collect();
    let email = fields
        .first()
        .copied()
        .unwrap_or_default()
        .trim()
        .to_string();
    if !outcome.ok() || email.is_empty() {
        return Err(io::Error::other("Google 계정을 읽지 못했습니다"));
    }

    let project = fields.get(1).copied().unwrap_or_default().trim();

    Ok(Observation {
        identity: ObservedIdentity::Google {
            email,
            // 프로젝트가 gcloud 와 firebase 를 가른다. 빈 값도 gcloud 임을 뜻해야 한다.
            project: Some(project.to_string()),
        },
        facts: AccountFacts {
            // OAuth 자격은 갱신 토큰으로 이어지므로 만료를 우리가 셀 수 없다.
            expires: Some(secrets_core::account::NEVER.to_string()),
            ..AccountFacts::default()
        },
    })
}

/// 안내 문장에서 계정 주소만 뽑는다.
///
/// firebase 는 신원을 문장으로 알려 준다. 문장을 통째로 이름으로 삼으면 계정
/// 이름이 안내 전문이 된다. `--json` 은 토큰까지 담아 오므로 쓰지 않는다.
fn address_in(text: &str) -> String {
    text.split_whitespace()
        .find(|token| token.contains('@'))
        .unwrap_or_default()
        .to_string()
}

/// firebase 는 `Logged in as tuk@tuk.im` 처럼 문장으로 알려 준다.
pub(super) fn probe_firebase(home_dir: &std::path::Path) -> io::Result<Observation> {
    let (outcome, raw) = capture(Provider::Firebase, home_dir, "firebase", &["login:list"])?;

    let email = address_in(&raw);

    if !outcome.ok() || email.is_empty() {
        return Err(io::Error::other("Firebase 계정을 읽지 못했습니다"));
    }

    Ok(Observation {
        identity: ObservedIdentity::Google {
            email,
            project: None,
        },
        facts: AccountFacts {
            expires: Some(secrets_core::account::NEVER.to_string()),
            ..AccountFacts::default()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identity_is_the_address_not_the_whole_sentence() {
        assert_eq!(address_in("Logged in as tuk@tuk.im"), "tuk@tuk.im");
        assert_eq!(address_in("✔ Logged in as a.b@c.co.kr\n"), "a.b@c.co.kr");
        assert_eq!(address_in("No authorized accounts"), "");
    }



}
