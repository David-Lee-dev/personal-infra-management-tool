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

use crate::account::{Account, Provider, env_for};
use crate::identity::{AccountFacts, AwsPrincipalKind, ObservedIdentity, Observation};
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

pub fn method(provider: Provider) -> Method {
    match provider {
        Provider::Github => Method {
            browser_code: false,
            fields: &[Field {
                key: "token",
                label: "개인 액세스 토큰",
                secret: true,
                help: "repo · read:org · admin:public_key 범위가 필요합니다",
                required: true,
            }],
            browser_login: false,
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
/// 신원 확인에는 리전이 필요 없다 — sts 와 iam 은 전역 서비스다. 그래서 폼에서
/// 묻지 않지만, 나중에 쓸 리전 의존 명령을 위해 설정에는 하나 적어 둔다.
pub const DEFAULT_REGION: &str = "ap-northeast-2";

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
    F: Fn(exec::Stream, String) + Sync,
{
    connect_into(account.provider, &account.cli_home(), values, on_line)
}

/// 지정한 CLI 홈에 로그인한다.
///
/// 계정을 만들기 전에 자격을 확인해 보려면 임시 홈이 필요하므로, 경로를 받는다.
pub fn connect_into<F>(
    provider: Provider,
    home_dir: &std::path::Path,
    values: &Values,
    on_line: F,
) -> io::Result<exec::Outcome>
where
    F: Fn(exec::Stream, String) + Sync,
{
    // 홈이 없으면 CLI 가 엉뚱한 곳에 쓴다. 먼저 보장한다.
    home::create_private(home_dir)?;

    match provider {
        Provider::Github => connect_github(home_dir, values, on_line),
        Provider::Aws => connect_aws(home_dir, values),
        Provider::Gcloud | Provider::Firebase => browser_login(provider, home_dir, on_line),
    }
}

/// 브라우저를 열어 로그인시키고 끝날 때까지 기다린다.
///
/// CLI 가 브라우저를 띄우고 localhost 로 결과를 받아 스스로 완료하므로,
/// 우리가 중간에 코드를 받아 넘길 필요가 없다. 사람이 브라우저에서 끝내는 동안
/// 이 호출은 막혀 있으므로 호출자는 별도 스레드에서 불러야 한다.
fn browser_login<F>(
    provider: Provider,
    home_dir: &std::path::Path,
    on_line: F,
) -> io::Result<exec::Outcome>
where
    F: Fn(exec::Stream, String) + Sync,
{
    let (tool, args): (_, &[&str]) = match provider {
        Provider::Gcloud => ("gcloud", &["auth", "login", "--brief"]),
        // --no-localhost 를 명시한다. 붙이지 않아도 출력이 TTY 가 아니면 같은
        // 흐름으로 빠지지만, 그러면 인증 페이지가 안내하는 명령과 우리가 실제로
        // 실행한 명령이 달라 사용자가 대조할 수 없다.
        Provider::Firebase => ("firebase", &["login", "--no-localhost"]),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "브라우저 로그인 대상이 아닙니다",
            ));
        }
    };

    let program = tools::find_in_path(tool).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("{tool} 를 찾을 수 없습니다"),
        )
    })?;

    exec::run_env(&program, args, &env_for(provider, home_dir), on_line)
}

fn connect_github<F>(
    home_dir: &std::path::Path,
    values: &Values,
    on_line: F,
) -> io::Result<exec::Outcome>
where
    F: Fn(exec::Stream, String) + Sync,
{
    let token = values.get("token").map(String::as_str).unwrap_or_default();
    let program = tools::find_in_path("gh")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "gh 를 찾을 수 없습니다"))?;

    // 토큰은 stdin 으로만 간다. argv 에 실으면 ps 로 읽힌다.
    exec::run_full(
        &program,
        &["auth", "login", "--with-token"],
        &env_for(Provider::Github, home_dir),
        Some(format!("{token}\n").as_bytes()),
        on_line,
    )
}

/// AWS 는 CLI 로그인이라는 개념이 없다. 설정 파일을 쓰는 것이 곧 연결이다.
///
/// `aws configure` 를 대화형으로 돌리는 대신 파일을 직접 쓴다. 값이 어디로
/// 가는지가 명확하고, 프롬프트 순서에 의존하지 않는다.
fn connect_aws(home_dir: &std::path::Path, values: &Values) -> io::Result<exec::Outcome> {
    let home = home_dir.to_path_buf();
    let get = |key: &str| {
        values
            .get(key)
            .map(String::as_str)
            .unwrap_or_default()
            .trim()
    };

    let region = match get("region") {
        "" => DEFAULT_REGION,
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
    fn parses_both_arn_shapes() {
        let (account, name, kind) = parse_arn("arn:aws:iam::320042238085:user/david-lee-admin");
        assert_eq!(account, "320042238085");
        assert_eq!(name, "david-lee-admin");
        assert_eq!(kind, AwsPrincipalKind::User);

        // 역할을 맡은 경우 세션 이름이 아니라 역할 이름이 신원이다.
        let (account, name, kind) =
            parse_arn("arn:aws:sts::320042238085:assumed-role/admin/my-session");
        assert_eq!(account, "320042238085");
        assert_eq!(name, "admin");
        assert_eq!(kind, AwsPrincipalKind::AssumedRole);
    }



    #[test]
    fn aws_writes_into_a_given_home() {
        with_temp_root(|_| {
            let account = Account::new(Provider::Aws, "tuk");
            account.save().unwrap();
            let scratch = home::Scratch::new("replace-test").unwrap();

            connect_into(
                Provider::Aws,
                scratch.path(),
                &values(&[("access_key_id", "AKIAX"), ("secret_access_key", "s")]),
                |_, _| {},
            )
            .unwrap();

            // 계정 홈이 아니라 지정한 곳에 쓰여야 한다. 확인 단계가 이걸 쓴다.
            assert!(scratch.path().join("credentials").is_file());
            assert!(!account.cli_home().join("credentials").exists());
        });
    }


    #[test]
    fn header_lookup_is_case_insensitive() {
        let raw = "HTTP/2.0 200 OK\nX-Oauth-Scopes: repo, read:org\nDate: x\n";
        assert_eq!(header(raw, "x-oauth-scopes"), Some("repo, read:org"));
        assert_eq!(header(raw, "X-OAUTH-SCOPES"), Some("repo, read:org"));
        assert_eq!(header(raw, "missing"), None);
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
            let method = method(provider);
            assert!(method.fields.is_empty(), "{provider:?}");
            assert!(method.browser_login, "{provider:?} 는 브라우저로 연결한다");
        }
        // 값을 받아 적는 provider 는 브라우저 로그인이 아니다.
        for provider in [Provider::Github, Provider::Aws] {
            assert!(!method(provider).browser_login, "{provider:?}");
        }
    }

    #[test]
    fn extracts_the_auth_url_from_cli_output() {
        let out = "To sign in:\n 1. session ID: BDEC1\n 2. Visit:\n   https://auth.firebase.tools/login?code_challenge=abc&session=xyz\n 3. run firebase login <code>";
        assert_eq!(
            first_url(out).unwrap(),
            "https://auth.firebase.tools/login?code_challenge=abc&session=xyz"
        );
        assert!(first_url("주소가 없는 출력").is_none());
    }

    #[test]
    fn finds_the_session_id_to_match_in_the_browser() {
        let note = "To sign in to the Firebase CLI:\n\n1. Take note of your session ID:\n\n   DA2F7\n\n2. Visit the URL below";
        let url = "https://auth.firebase.tools/login?session=da2f7141-e311-4917";
        assert_eq!(session_id(note, url), "DA2F7");

        // 출력 형식이 바뀌어도 주소에서 같은 값을 뽑는다.
        assert_eq!(session_id("안내가 달라졌다", url), "DA2F7");
        assert_eq!(session_id("", "주소도 없다"), "");
    }

    #[test]
    fn only_firebase_needs_a_code_pasted_back() {
        assert!(method(Provider::Firebase).browser_code);
        // gcloud 는 localhost 로 결과를 받아 스스로 끝낸다.
        assert!(!method(Provider::Gcloud).browser_code);
        assert!(!method(Provider::Github).browser_code);
    }

    #[test]
    fn firebase_identity_is_the_address_not_the_sentence() {
        // `Logged in as tuk@tuk.im` 전체가 이름으로 기록된 적이 있다.
        let pick = |raw: &str| {
            raw.split_whitespace()
                .find(|t| t.contains('@'))
                .unwrap_or_default()
                .to_string()
        };
        assert_eq!(pick("Logged in as tuk@tuk.im"), "tuk@tuk.im");
        assert_eq!(pick("✔ Logged in as a.b@c.co.kr\n"), "a.b@c.co.kr");
        assert_eq!(pick("No authorized accounts"), "");
    }


    #[test]
    fn aws_asks_only_for_the_credential() {
        let fields = method(Provider::Aws).fields;
        // 리전은 자격이 아니라 설정이다. 신원 확인(sts·iam)은 전역 서비스라
        // 리전 없이 되므로 폼에서 묻지 않는다.
        assert_eq!(fields.len(), 2);
        assert!(
            fields.iter().all(|f| f.required),
            "둘 다 없으면 연결이 안 된다"
        );
        assert!(
            fields
                .iter()
                .any(|f| f.key == "secret_access_key" && f.secret)
        );
    }

    #[test]
    fn region_falls_back_to_a_default_when_not_given() {
        with_temp_root(|_| {
            let account = Account::new(Provider::Aws, "tuk");
            account.save().unwrap();

            connect(
                &account,
                &values(&[("access_key_id", "AKIAX"), ("secret_access_key", "s")]),
                |_, _| {},
            )
            .unwrap();

            let config = std::fs::read_to_string(account.cli_home().join("config")).unwrap();
            assert!(config.contains(DEFAULT_REGION), "{config}");
        });
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

/// 주어진 홈에 입력값으로 로그인하고 신원을 읽는다.
///
/// 로그인 결과는 이 홈에 남는다. 호출자가 그 홈을 계정 홈으로 그대로 옮기므로
/// 같은 자격으로 두 번 로그인할 일이 없다.
pub fn probe_in<F>(
    provider: Provider,
    home_dir: &std::path::Path,
    values: &Values,
    on_line: F,
) -> io::Result<Observation>
where
    F: Fn(exec::Stream, String) + Sync,
{
    validate(provider, values).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let outcome = connect_into(provider, home_dir, values, on_line)?;
    if !outcome.ok() {
        return Err(io::Error::other("자격으로 로그인하지 못했습니다"));
    }
    probe_home(provider, home_dir)
}

/// 주어진 홈에서 브라우저로 로그인시키고 신원을 읽는다. 한 번에 끝나는 provider 용.
///
/// 로그인 결과는 이 홈에 남는다. 호출자가 계정 홈으로 옮기므로 브라우저를 두 번
/// 띄우지 않는다.
pub fn browser_probe_in<F>(
    provider: Provider,
    home_dir: &std::path::Path,
    on_line: F,
) -> io::Result<Observation>
where
    F: Fn(exec::Stream, String) + Sync,
{
    home::create_private(home_dir)?;

    let outcome = browser_login(provider, home_dir, on_line)?;
    if !outcome.ok() {
        return Err(io::Error::other("브라우저 로그인이 완료되지 않았습니다"));
    }
    probe_home(provider, home_dir)
}

/// 코드를 받아 와야 끝나는 로그인의 첫 단계.
#[derive(Debug, Clone)]
pub struct Challenge {
    /// 사람이 열어야 할 주소.
    pub url: String,
    /// 브라우저 페이지에서 대조할 세션 번호.
    ///
    /// 탭이 여러 개 떠 있으면 다른 세션의 코드를 붙여넣기 쉽다. 그러면 서버가
    /// 코드를 거부하는데 이유가 드러나지 않는다. 대조할 수 있게 보여 준다.
    pub session: String,
    /// CLI 가 알려 준 안내 전문.
    pub note: String,
}

/// 주어진 홈에서 로그인을 시작해 인증 주소를 받아 온다.
///
/// 두 번째 단계가 같은 홈을 써야 세션이 이어진다.
pub fn browser_begin_in<F>(
    provider: Provider,
    stage: &std::path::Path,
    on_line: F,
) -> io::Result<Challenge>
where
    F: Fn(exec::Stream, String) + Sync,
{
    home::create_private(stage)?;

    let program = tools::find_in_path("firebase")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "firebase 를 찾을 수 없습니다"))?;

    let buffer = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = buffer.clone();

    exec::run_env(
        &program,
        &["login", "--no-localhost"],
        &env_for(provider, stage),
        move |stream, line| {
            if let Ok(mut buf) = sink.lock() {
                buf.push_str(&line);
                buf.push('\n');
            }
            on_line(stream, line);
        },
    )?;

    let note = buffer.lock().map(|b| b.clone()).unwrap_or_default();
    let url = first_url(&note).ok_or_else(|| io::Error::other("인증 주소를 찾지 못했습니다"))?;

    let session = session_id(&note, &url);
    Ok(Challenge { url, session, note })
}

/// 안내 전문에서 세션 번호를 찾는다.
///
/// CLI 가 `session ID:` 다음 줄에 찍어 주고, 그 값은 주소의 session 앞부분이다.
/// 출력 형식이 바뀌어도 주소에서 뽑을 수 있게 두 갈래로 둔다.
fn session_id(note: &str, url: &str) -> String {
    let lines: Vec<&str> = note.lines().map(str::trim).collect();
    if let Some(i) = lines.iter().position(|l| l.contains("session ID"))
        && let Some(found) = lines[i + 1..].iter().find(|l| !l.is_empty())
    {
        return (*found).to_string();
    }

    url.split("session=")
        .nth(1)
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .take(5)
                .collect::<String>()
                .to_ascii_uppercase()
        })
        .unwrap_or_default()
}

/// 브라우저에서 받은 코드로 로그인을 끝낸다.
pub fn browser_complete_in<F>(
    provider: Provider,
    stage: &std::path::Path,
    code: &str,
    on_line: F,
) -> io::Result<Observation>
where
    F: Fn(exec::Stream, String) + Sync,
{
    let code = code.trim();
    if code.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "코드를 입력하세요",
        ));
    }

    if !stage.is_dir() {
        return Err(io::Error::other("로그인을 먼저 시작하세요"));
    }

    let program = tools::find_in_path("firebase")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "firebase 를 찾을 수 없습니다"))?;

    // 첫 단계와 같은 설정 홈이어야 한다. 세션과 검증자가 거기 들어 있다.
    let buffer = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = buffer.clone();

    let outcome = exec::run_env(
        &program,
        &["login", code],
        &env_for(provider, stage),
        move |stream, line| {
            if let Ok(mut buf) = sink.lock() {
                buf.push_str(&line);
                buf.push('\n');
            }
            on_line(stream, line);
        },
    )?;

    if !outcome.ok() {
        // CLI 가 말한 이유를 그대로 전한다. 우리 말로 바꾸면 원인을 잃는다.
        let detail = buffer
            .lock()
            .ok()
            .and_then(|b| {
                b.lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_default();

        return Err(io::Error::other(format!(
            "코드로 로그인하지 못했습니다. 코드는 몇 분 안에 만료되니 다시 로그인해 새 코드를 받으세요. ({detail})"
        )));
    }
    probe_home(provider, stage)
}

/// 출력에서 첫 번째 https 주소를 뽑는다.
fn first_url(text: &str) -> Option<String> {
    let start = text.find("https://")?;
    let rest = &text[start..];
    // 공백이나 줄바꿈에서 끊는다. CLI 가 주소 뒤에 안내를 붙이는 경우가 있다.
    let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// 이미 로그인된 홈에서 신원을 읽는다.
///
/// 브라우저 로그인은 확인 단계가 따로 없다 — 로그인 자체가 확인이므로,
/// 로그인이 끝난 뒤 그 홈을 그대로 읽는다.
pub fn probe_home(provider: Provider, home_dir: &std::path::Path) -> io::Result<Observation> {
    probe_home_logging(provider, home_dir, |_, _| {})
}

/// gcloud 는 설정에서 계정과 기본 프로젝트를 읽는다.
fn probe_gcloud(home_dir: &std::path::Path) -> io::Result<Observation> {
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
            expires: Some(crate::account::NEVER.to_string()),
            ..AccountFacts::default()
        },
    })
}

/// firebase 는 `Logged in as tuk@tuk.im` 처럼 문장으로 알려 준다.
fn probe_firebase(home_dir: &std::path::Path) -> io::Result<Observation> {
    let (outcome, raw) = capture(Provider::Firebase, home_dir, "firebase", &["login:list"])?;

    // 안내 전문이 아니라 주소만 남긴다. --json 은 토큰까지 담아 오므로 쓰지 않는다.
    let email = raw
        .split_whitespace()
        .find(|token| token.contains('@'))
        .unwrap_or_default()
        .to_string();

    if !outcome.ok() || email.is_empty() {
        return Err(io::Error::other("Firebase 계정을 읽지 못했습니다"));
    }

    Ok(Observation {
        identity: ObservedIdentity::Google {
            email,
            project: None,
        },
        facts: AccountFacts {
            expires: Some(crate::account::NEVER.to_string()),
            ..AccountFacts::default()
        },
    })
}

/// 신원을 읽으면서 실행 명령을 터미널에도 보여 준다.
pub fn probe_home_logging<F>(
    provider: Provider,
    home_dir: &std::path::Path,
    on_line: F,
) -> io::Result<Observation>
where
    F: Fn(exec::Stream, String) + Sync,
{
    // 어떤 명령이 나갔는지는 보이게 하되, 출력 해석은 provider 별 함수에 맡긴다.
    on_line(exec::Stream::Stdout, format!("{} 신원 확인", provider.id()));

    match provider {
        Provider::Github => probe_github(home_dir),
        Provider::Aws => probe_aws(home_dir),
        Provider::Gcloud => probe_gcloud(home_dir),
        Provider::Firebase => probe_firebase(home_dir),
    }
}

/// 헤더 이름으로 값을 찾는다. 이름의 대소문자는 서버마다 다르다.
fn header<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let wanted = format!("{}:", name.to_ascii_lowercase());
    text.lines()
        .find(|line| line.to_ascii_lowercase().starts_with(&wanted))
        .and_then(|line| line.split_once(':'))
        .map(|(_, value)| value.trim())
}

/// 지정한 홈에서 CLI 를 돌리고 출력을 통째로 받는다.
fn capture(
    provider: Provider,
    home_dir: &std::path::Path,
    tool: &str,
    args: &[&str],
) -> io::Result<(exec::Outcome, String)> {
    let program = tools::find_in_path(tool).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("{tool} 를 찾을 수 없습니다"),
        )
    })?;

    let buffer = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = buffer.clone();

    let outcome = exec::run_env(
        &program,
        args,
        &env_for(provider, home_dir),
        move |_, line| {
            if let Ok(mut buf) = sink.lock() {
                buf.push_str(&line);
                buf.push('\n');
            }
        },
    )?;

    let text = buffer.lock().map(|b| b.clone()).unwrap_or_default();
    Ok((outcome, text))
}

/// GitHub 은 신원과 함께 토큰의 권한·만료일까지 헤더로 알려 준다.
fn probe_github(home_dir: &std::path::Path) -> io::Result<Observation> {
    // id 는 noreply 이메일을 만드는 데 쓴다. email 은 비공개면 비어서 온다.
    let (outcome, raw) = capture(
        Provider::Github,
        home_dir,
        "gh",
        &[
            "api",
            "user",
            "--jq",
            r#""\(.login)\t\(.id)\t\(.email // "")""#,
        ],
    )?;

    let fields: Vec<&str> = raw.trim().split('\t').collect();
    let login = fields.first().copied().unwrap_or_default().to_string();
    if !outcome.ok() || login.is_empty() {
        return Err(io::Error::other("GitHub 로그인 이름을 읽지 못했습니다"));
    }

    let (_, headers) = capture(Provider::Github, home_dir, "gh", &["api", "user", "-i"])?;

    // `2026-12-21 05:00:00 UTC` 형태로 온다. 날짜 부분만 쓴다.
    // 헤더가 없으면 기한 없는 토큰이다.
    let expires = match header(&headers, "github-authentication-token-expiration") {
        Some(raw) => raw
            .split_whitespace()
            .next()
            .filter(|d| crate::date::parse(d).is_some())
            .map(str::to_string),
        None => Some(crate::account::NEVER.to_string()),
    };

    Ok(Observation {
        identity: ObservedIdentity::Github {
            login,
            user_id: fields.get(1).copied().unwrap_or_default().to_string(),
            public_email: fields.get(2).map(|e| e.trim().to_string()),
        },
        facts: AccountFacts {
            expires,
            scopes: header(&headers, "x-oauth-scopes")
                .map(|raw| {
                    raw.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            ..AccountFacts::default()
        },
    })
}

/// AWS 는 신원 외에 **마스터 계정 자격이 있는지**까지 본다.
fn probe_aws(home_dir: &std::path::Path) -> io::Result<Observation> {
    let (outcome, arn) = capture(
        Provider::Aws,
        home_dir,
        "aws",
        &[
            "sts",
            "get-caller-identity",
            "--query",
            "Arn",
            "--output",
            "text",
        ],
    )?;
    let arn = arn.trim().to_string();
    if !outcome.ok() || arn.is_empty() {
        return Err(io::Error::other("AWS 신원을 읽지 못했습니다"));
    }

    let (account_id, principal_name, principal_kind) = parse_arn(&arn);
    if principal_name.is_empty() {
        return Err(io::Error::other(format!(
            "신원을 해석하지 못했습니다: {arn}"
        )));
    }

    // 계정 별칭이 있으면 번호보다 알아보기 쉽다. 읽을 권한이 없으면 조용히 넘어간다 —
    // 권한이 없는 것은 실패가 아니다.
    let alias = capture(
        Provider::Aws,
        home_dir,
        "aws",
        &[
            "iam",
            "list-account-aliases",
            "--query",
            "AccountAliases[0]",
            "--output",
            "text",
        ],
    )
    .ok()
    .filter(|(o, _)| o.ok())
    .map(|(_, t)| t.trim().to_string())
    .filter(|t| !t.is_empty() && t != "None");

    // 마스터 계정은 자격을 발급할 수 있어야 한다. IAM 계정 정보를 못 읽는
    // 신원은 발급도 못 하므로 여기서 막는다 — 권한이 한정된 작업용 IAM 사용자가
    // 마스터 계정으로 들어앉으면 회전도 발급도 안 되는 껍데기가 된다.
    let summary = capture(
        Provider::Aws,
        home_dir,
        "aws",
        &[
            "iam",
            "get-account-summary",
            "--query",
            "[SummaryMap.AccountAccessKeysPresent, SummaryMap.AccountMFAEnabled]",
            "--output",
            "text",
        ],
    );

    let Some((_, summary)) = summary.ok().filter(|(o, _)| o.ok()) else {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{principal_name} 는 IAM 계정 정보를 읽을 수 없습니다. 마스터 계정은 관리자 권한이 필요합니다"
            ),
        ));
    };

    // `0\t1` 형태로 온다.
    let flags: Vec<&str> = summary.split_whitespace().collect();
    let flag = |i: usize| flags.get(i).map(|v| *v == "1");

    Ok(Observation {
        identity: ObservedIdentity::Aws {
            arn,
            account_id,
            principal_name,
            principal_kind,
            alias,
        },
        facts: AccountFacts {
            // 액세스 키에는 기한이 없다. 회전은 정책으로 한다.
            expires: Some(crate::account::NEVER.to_string()),
            root_keys_present: flag(0),
            root_mfa: flag(1),
            ..AccountFacts::default()
        },
    })
}

/// ARN 에서 계정 번호와 주체를 뽑는다.
///
/// `arn:aws:iam::320042238085:user/david` 또는
/// `arn:aws:sts::320042238085:assumed-role/admin/session` 형태로 온다.
fn parse_arn(arn: &str) -> (String, String, AwsPrincipalKind) {
    let parts: Vec<&str> = arn.split(':').collect();
    let account_id = parts.get(4).copied().unwrap_or_default().to_string();
    let resource = parts.get(5).copied().unwrap_or_default();

    let kind = if resource.starts_with("assumed-role/") {
        AwsPrincipalKind::AssumedRole
    } else {
        AwsPrincipalKind::User
    };

    // 역할을 맡은 경우 세션 이름이 아니라 역할 이름이 신원이다.
    let name = match kind {
        AwsPrincipalKind::AssumedRole => resource.split('/').nth(1).unwrap_or_default(),
        AwsPrincipalKind::User => resource.rsplit('/').next().unwrap_or_default(),
    };

    (account_id, name.to_string(), kind)
}


