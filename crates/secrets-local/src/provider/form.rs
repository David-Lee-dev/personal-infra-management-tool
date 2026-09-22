//! provider 를 연결하려면 무엇을 받아 적어야 하는가.
//!
//! 자격의 구성은 provider 마다 다르다. 화면은 이 명세를 보고 폼을 그린다.

use secrets_core::account::Provider;

use super::github::{github_fields, github_token_url};

/// 입력 칸 하나.
#[derive(Debug, Clone, Copy)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    /// 가려서 입력받아야 하는가.
    pub secret: bool,
    /// 입력칸 아래 보여줄 설명.
    pub help: &'static str,
    pub required: bool,
}

/// 값을 받지 않고 브라우저에서 처리해야 하는 경우, 열어줄 주소.
#[derive(Debug, Clone, Copy)]
pub struct Browser {
    pub label: &'static str,
    pub url: &'static str,
}

/// provider 를 연결하는 방법.
#[derive(Debug, Clone, Copy)]
pub struct Method {
    pub fields: &'static [Field],
    /// 입력 대신 브라우저 로그인으로 연결하는가.
    ///
    /// 받아 적을 비밀값이 없는 provider 가 있다.
    pub browser_login: bool,
    /// 브라우저에서 받은 코드를 되돌려 넣어야 끝나는가.
    ///
    /// gcloud 는 브라우저를 열고 localhost 로 결과를 받아 스스로 끝낸다.
    /// firebase 는 출력이 TTY 가 아니면 URL 과 코드 입력을 요구하는 흐름으로
    /// 빠지므로, 두 단계로 나눠야 한다.
    pub browser_code: bool,
    /// 값을 얻으러 갈 곳. 폼 옆에 링크로 띄운다.
    pub browser: Option<Browser>,
    /// 사용자에게 보여줄 안내.
    pub guidance: &'static str,
}

/// 이 provider 를 다루는 CLI 의 레지스트리 id.
///
/// 어떤 명령줄 도구로 그 provider 를 다루는지는 core 가 알 일이 아니다.
pub fn method(provider: Provider) -> Method {
    match provider {
        Provider::Github => Method {
            browser_code: false,
            fields: github_fields(),
            browser_login: false,
            browser: Some(Browser {
                label: "GitHub 에서 토큰 발급",
                url: github_token_url(),
            }),
            guidance: "GitHub 은 비밀번호로 CLI 인증을 받지 않습니다. 토큰을 발급해 붙여넣고 자격 확인을 누르면 계정 이름과 만료일을 읽어 옵니다.",
        },
        Provider::Aws => Method {
            browser_code: false,
            fields: &[
                Field {
                    key: "access_key_id",
                    label: "Access Key ID",
                    secret: false,
                    help: "AKIA 로 시작하는 20자",
                    required: true,
                },
                Field {
                    key: "secret_access_key",
                    label: "Secret Access Key",
                    secret: true,
                    help: "발급 시 한 번만 보여집니다",
                    required: true,
                },
            ],
            browser_login: false,
            browser: None,
            guidance: "관리자 권한 IAM 사용자의 액세스 키를 입력하세요. 마스터 계정은 자격을 발급할 수 있어야 하므로 권한이 한정된 사용자는 등록되지 않습니다. root 자격은 넣지 마세요 — 권한을 좁힐 수 없어 이 도구가 다루지 않습니다.",
        },
        Provider::Gcloud => Method {
            browser_code: false,
            fields: &[],
            browser_login: true,
            browser: None,
            guidance: "브라우저가 열립니다. Google 계정으로 로그인하면 이 계정 전용 설정에만 기록되고, 지금 쓰고 있는 로그인은 그대로 남습니다.",
        },
        Provider::Firebase => Method {
            browser_code: true,
            fields: &[],
            browser_login: true,
            browser: None,
            guidance: "브라우저가 열립니다. Google 계정으로 로그인하면 이 계정 전용 설정에만 기록되고, 지금 쓰고 있는 로그인은 그대로 남습니다.",
        },
    }
}

/// 리전을 적지 않았을 때 쓸 값.
///
/// 입력값 한 묶음. 키는 `Field::key`.
pub type Values = std::collections::HashMap<String, String>;

/// 폼이 요구하는 값이 다 왔는지 확인한다.
pub fn validate(provider: Provider, values: &Values) -> Result<(), String> {
    for field in method(provider).fields {
        if field.required
            && values
                .get(field.key)
                .map(|v| v.trim().is_empty())
                .unwrap_or(true)
        {
            return Err(format!("{} 을(를) 입력하세요", field.label));
        }
    }
    Ok(())
}
