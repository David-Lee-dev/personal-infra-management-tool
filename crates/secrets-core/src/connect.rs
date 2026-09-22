//! 계정 연결 — provider 마다 무엇을 입력받아 어떻게 로그인시키는가.
//!
//! provider 별로 받을 수 있는 값이 다르다. 아무거나 id/pw 로 뭉뚱그리면
//! 쓰지도 못할 값을 보관하게 된다.
//!
//! - GitHub  : 개인 액세스 토큰. 비밀번호 인증은 2021 년에 폐지됐다.
//! - AWS     : 액세스 키 ID + 시크릿. 유일하게 폼이 그대로 맞는 provider.
//! - GCP·Firebase : 브라우저 OAuth 만 가능하다. 입력받을 값이 없다.
//!
//! 비밀값은 언제나 stdin 이나 파일로만 넘어간다. 명령행 인자로 넘기지 않는다.

use std::io;

use crate::account::{Account, Provider};
use crate::{exec, home, tools};

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
    /// 값을 얻으러 갈 곳. 폼 옆에 링크로 띄운다.
    pub browser: Option<Browser>,
    /// 사용자에게 보여줄 안내.
    pub guidance: &'static str,
}

pub fn method(provider: Provider) -> Method {
    match provider {
        Provider::Github => Method {
            fields: &[Field {
                key: "token",
                label: "개인 액세스 토큰",
                secret: true,
                help: "repo · read:org · admin:public_key 범위가 필요합니다",
                required: true,
            }],
            browser: Some(Browser {
                label: "GitHub 에서 토큰 발급",
                // scope 는 이 도구가 실제로 호출하는 것만 담는다.
                //   repo                  — deploy key 등록·삭제
                //   admin:org             — 조직 리포 접근 (사용자 선택)
                //   admin:public_key      — 계정 SSH 키 등록·삭제
                //   admin:gpg_key         — GPG 키 등록·삭제
                //   admin:ssh_signing_key — SSH 서명 키 등록·삭제
                // delete 까지 하려면 write:* 가 아니라 admin:* 이어야 한다.
                url: "https://github.com/settings/tokens/new?scopes=repo,admin:org,admin:public_key,admin:gpg_key,admin:ssh_signing_key&description=secrets-manager",
            }),
            guidance: "GitHub 은 비밀번호로 CLI 인증을 받지 않습니다. 토큰을 발급해 붙여넣으세요. 만료일을 함께 적어 두면 기한이 다가올 때 알려 드립니다.",
        },
        Provider::Aws => Method {
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
                Field {
                    key: "region",
                    label: "기본 리전",
                    secret: false,
                    help: "예: ap-northeast-2",
                    required: false,
                },
            ],
            browser: None,
            guidance: "IAM 사용자의 액세스 키를 입력합니다. 이 계정 전용 설정 파일에만 기록됩니다.",
        },
        Provider::Gcloud => Method {
            fields: &[],
            browser: None,
            guidance: "Google Cloud 는 브라우저 로그인만 지원합니다. 입력받을 값이 없습니다.",
        },
        Provider::Firebase => Method {
            fields: &[],
            browser: None,
            guidance: "Firebase 는 브라우저 로그인만 지원합니다. 입력받을 값이 없습니다.",
        },
    }
}

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

/// 입력값으로 실제 로그인을 수행한다.
///
/// 로그인은 계정 전용 CLI 홈 안에서만 일어난다. 기존 로그인은 건드리지 않는다.
pub fn connect<F>(account: &Account, values: &Values, on_line: F) -> io::Result<exec::Outcome>
where
    F: Fn(exec::Stream, String) + Send + Sync + 'static,
{
    // 홈이 없으면 CLI 가 엉뚱한 곳에 쓴다. 먼저 보장한다.
    home::create_private(&account.cli_home())?;

    match account.provider {
        Provider::Github => connect_github(account, values, on_line),
        Provider::Aws => connect_aws(account, values),
        Provider::Gcloud | Provider::Firebase => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "브라우저 로그인은 아직 지원하지 않습니다",
        )),
    }
}

fn connect_github<F>(account: &Account, values: &Values, on_line: F) -> io::Result<exec::Outcome>
where
    F: Fn(exec::Stream, String) + Send + Sync + 'static,
{
    let token = values.get("token").map(String::as_str).unwrap_or_default();
    let program = tools::find_in_path("gh")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "gh 를 찾을 수 없습니다"))?;

    // 토큰은 stdin 으로만 간다. argv 에 실으면 ps 로 읽힌다.
    exec::run_full(
        &program,
        &["auth", "login", "--with-token"],
        &account.env(),
        Some(format!("{token}\n").as_bytes()),
        on_line,
    )
}

/// AWS 는 CLI 로그인이라는 개념이 없다. 설정 파일을 쓰는 것이 곧 연결이다.
///
/// `aws configure` 를 대화형으로 돌리는 대신 파일을 직접 쓴다. 값이 어디로
/// 가는지가 명확하고, 프롬프트 순서에 의존하지 않는다.
fn connect_aws(account: &Account, values: &Values) -> io::Result<exec::Outcome> {
    let home = account.cli_home();
    let get = |key: &str| {
        values
            .get(key)
            .map(String::as_str)
            .unwrap_or_default()
            .trim()
    };

    let region = match get("region") {
        "" => "ap-northeast-2",
        value => value,
    };

    // 계정마다 파일이 따로이므로 프로필 이름은 default 로 고정한다.
    // 어느 계정인지는 파일 경로가 말해 준다.
    std::fs::write(
        home.join("config"),
        format!("[default]\nregion = {region}\noutput = json\n"),
    )?;
    home::restrict(&home.join("config"))?;

    std::fs::write(
        home.join("credentials"),
        format!(
            "[default]\naws_access_key_id = {}\naws_secret_access_key = {}\n",
            get("access_key_id"),
            get("secret_access_key")
        ),
    )?;
    home::restrict(&home.join("credentials"))?;

    Ok(exec::Outcome { code: Some(0) })
}

/// 검증 결과 — 이 계정이 실제로 누구인가.
#[derive(Debug, Clone)]
pub struct Whoami {
    pub ok: bool,
    /// iam-user, oauth 등.
    pub kind: String,
    /// 로그인 이름 · ARN 등.
    pub name: String,
    /// 판정 근거. 실패 원인을 남긴다.
    pub detail: String,
}

/// 계정 전용 설정 홈으로 CLI 를 돌려 실제 신원을 확인한다.
///
/// 선언을 믿지 않고 매번 실제로 물어본다. 연결 직후에도, 나중에도 같은 함수를 쓴다.
pub fn verify<F>(account: &Account, on_line: F) -> io::Result<Whoami>
where
    F: Fn(exec::Stream, String) + Send + Sync + 'static,
{
    let (tool, args, kind) = match account.provider {
        Provider::Github => ("gh", vec!["api", "user", "--jq", ".login"], "oauth"),
        Provider::Aws => (
            "aws",
            vec![
                "sts",
                "get-caller-identity",
                "--query",
                "Arn",
                "--output",
                "text",
            ],
            "iam",
        ),
        Provider::Gcloud => (
            "gcloud",
            vec!["config", "list", "--format=value(core.account)"],
            "oauth",
        ),
        Provider::Firebase => ("firebase", vec!["login:list"], "oauth"),
    };

    let program = tools::find_in_path(tool).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("{tool} 를 찾을 수 없습니다"),
        )
    })?;

    let collected = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = collected.clone();

    let outcome = exec::run_env(&program, &args, &account.env(), move |stream, line| {
        if let Ok(mut buf) = sink.lock() {
            buf.push_str(&line);
            buf.push('\n');
        }
        on_line(stream, line);
    })?;

    let output = collected.lock().map(|b| b.clone()).unwrap_or_default();
    let name = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .to_string();

    // 출력이 비어 있으면 성공 코드라도 신원을 못 읽은 것이다.
    let ok = outcome.ok() && !name.is_empty();
    Ok(Whoami {
        ok,
        kind: if ok { kind.to_string() } else { String::new() },
        name: if ok { name } else { String::new() },
        detail: if ok {
            String::new()
        } else {
            format!(
                "{} 로 신원을 확인하지 못했습니다",
                exec::display(tool, &args)
            )
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home::tests_support::with_temp_root;

    fn values(pairs: &[(&str, &str)]) -> Values {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn github_asks_for_a_token_not_a_password() {
        let fields = method(Provider::Github).fields;
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].key, "token");
        assert!(fields[0].secret);
    }

    #[test]
    fn browser_only_providers_have_no_fields() {
        for provider in [Provider::Gcloud, Provider::Firebase] {
            assert!(method(provider).fields.is_empty(), "{:?}", provider);
        }
    }

    #[test]
    fn validation_reports_the_missing_field_by_label() {
        let err = validate(Provider::Aws, &values(&[("access_key_id", "AKIA")])).unwrap_err();
        assert!(err.contains("Secret Access Key"), "{err}");

        assert!(
            validate(
                Provider::Aws,
                &values(&[("access_key_id", "AKIA"), ("secret_access_key", "x"),]),
            )
            .is_ok(),
            "리전은 선택 항목이다"
        );
    }

    #[test]
    fn aws_writes_into_the_accounts_own_files() {
        with_temp_root(|_| {
            let account = Account::new(Provider::Aws, "tuk");
            account.save().unwrap();

            connect(
                &account,
                &values(&[
                    ("access_key_id", "AKIAEXAMPLE"),
                    ("secret_access_key", "s3cret"),
                    ("region", "us-east-1"),
                ]),
                |_, _| {},
            )
            .unwrap();

            let creds = std::fs::read_to_string(account.cli_home().join("credentials")).unwrap();
            assert!(creds.contains("AKIAEXAMPLE"));

            let config = std::fs::read_to_string(account.cli_home().join("config")).unwrap();
            assert!(config.contains("us-east-1"));

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(account.cli_home().join("credentials"))
                    .unwrap()
                    .permissions()
                    .mode();
                assert_eq!(mode & 0o777, 0o600, "시크릿 파일은 0600 이어야 한다");
            }
        });
    }
}
