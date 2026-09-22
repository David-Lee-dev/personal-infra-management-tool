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
use std::path::PathBuf;

use crate::account::{Account, Provider, env_for};
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
    F: Fn(exec::Stream, String) + Send + Sync + 'static,
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
    F: Fn(exec::Stream, String) + Send + Sync + 'static,
{
    // 홈이 없으면 CLI 가 엉뚱한 곳에 쓴다. 먼저 보장한다.
    home::create_private(home_dir)?;

    match provider {
        Provider::Github => connect_github(home_dir, values, on_line),
        Provider::Aws => connect_aws(home_dir, values),
        Provider::Gcloud | Provider::Firebase => {
            // 확인 단계에서 이미 로그인했다면 그것을 쓴다. 같은 일로 브라우저를
            // 두 번 띄우지 않는다.
            if adopt_staged(provider, home_dir)? {
                return Ok(exec::Outcome { code: Some(0) });
            }
            browser_login(provider, home_dir, on_line)
        }
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
    F: Fn(exec::Stream, String) + Send + Sync + 'static,
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
    F: Fn(exec::Stream, String) + Send + Sync + 'static,
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
    verify_in(account.provider, &account.cli_home(), on_line)
}

/// 지정한 CLI 홈으로 신원을 확인한다.
pub fn verify_in<F>(
    provider: Provider,
    home_dir: &std::path::Path,
    on_line: F,
) -> io::Result<Whoami>
where
    F: Fn(exec::Stream, String) + Send + Sync + 'static,
{
    // 신원을 읽는 규칙은 provider 마다 다르다. 그 규칙을 여기 한 번 더 적으면
    // 확인 단계와 어긋난다 — 실제로 firebase 의 `Logged in as x@y` 라는 안내
    // 전문이 통째로 이름에 들어간 적이 있다. 확인 단계와 같은 함수를 쓴다.
    match probe_home_logging(provider, home_dir, on_line) {
        Ok(probe) => Ok(Whoami {
            ok: true,
            kind: probe.kind,
            name: probe.name,
            detail: String::new(),
        }),
        Err(e) => Ok(Whoami {
            ok: false,
            kind: String::new(),
            name: String::new(),
            detail: format!("신원을 확인하지 못했습니다: {e}"),
        }),
    }
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
        assert_eq!(
            parse_arn("arn:aws:iam::320042238085:user/david-lee-admin"),
            ("320042238085".into(), "david-lee-admin".into())
        );
        // 역할을 맡은 경우 세션 이름이 아니라 역할 이름이 신원이다.
        assert_eq!(
            parse_arn("arn:aws:sts::320042238085:assumed-role/Deployer/session-1"),
            ("320042238085".into(), "Deployer".into())
        );
        // 해석 못 하는 값은 빈 이름으로 돌려 호출자가 막게 한다.
        assert_eq!(parse_arn("이건 arn 이 아니다").1, "");
    }

    #[test]
    fn same_aws_account_different_users_get_different_slugs() {
        // 한 AWS 계정에 사용자가 여럿이면 계정 번호로는 구분되지 않는다.
        let a = parse_arn("arn:aws:iam::320042238085:user/david-lee-admin").1;
        let b = parse_arn("arn:aws:iam::320042238085:user/tuk-dev-power").1;
        assert_ne!(slugify(&a), slugify(&b));
        assert_eq!(slugify(&a), "david-lee-admin");
    }

    #[test]
    fn refuses_a_credential_from_another_account() {
        assert!(same_account("David-Lee-dev", "David-Lee-dev").is_ok());

        let err = same_account("David-Lee-dev", "SomeoneElse").unwrap_err();
        assert!(err.contains("David-Lee-dev"), "{err}");
        assert!(err.contains("SomeoneElse"), "{err}");

        // 대소문자가 다르면 다른 계정이다. GitHub 로그인은 대소문자를 보존한다.
        assert!(same_account("David-Lee-dev", "david-lee-dev").is_err());

        // 아직 검증한 적 없는 계정은 비교할 대상이 없다.
        assert!(same_account("", "누구든").is_ok());
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
    fn slugify_follows_the_slug_rules() {
        for (input, expected) in [
            ("David-Lee-dev", "david-lee-dev"),
            ("tuk_prod", "tuk-prod"),
            ("My Org!!", "my-org"),
            ("---x---", "x"),
            ("123456789012", "123456789012"),
        ] {
            let slug = slugify(input);
            assert_eq!(slug, expected, "입력: {input}");
            assert!(crate::account::validate_slug(&slug).is_ok(), "{slug}");
        }
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
    fn staged_login_is_moved_into_the_account_home() {
        with_temp_root(|_| {
            let account = Account::new(Provider::Gcloud, "tuk");
            account.save().unwrap();

            // 확인 단계가 남겨 둔 로그인이 있다고 하자.
            let stage = staging(Provider::Gcloud);
            home::create_private(&stage).unwrap();
            std::fs::write(stage.join("credentials.db"), "로그인").unwrap();

            assert!(adopt_staged(Provider::Gcloud, &account.cli_home()).unwrap());

            assert_eq!(
                std::fs::read_to_string(account.cli_home().join("credentials.db")).unwrap(),
                "로그인",
                "확인 때 받은 자격을 그대로 써야 브라우저를 두 번 띄우지 않는다"
            );
            assert!(!stage.exists(), "staging 은 비워진다");

            // 두 번째 호출은 옮길 것이 없다.
            assert!(!adopt_staged(Provider::Gcloud, &account.cli_home()).unwrap());
        });
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
    fn email_slugs_stay_within_the_rules() {
        // gcloud·firebase 는 이메일이 신원이다. 그대로 두면 슬러그가 될 수 없다.
        let slug = slugify("tuk@tuk.im");
        assert_eq!(slug, "tuk-tuk-im");
        assert!(crate::account::validate_slug(&slug).is_ok());
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

/// 자격만 가지고 알아낸 계정 정보.
///
/// 계정을 만들기 전에 돌린다. 사람이 이름과 만료일을 추측해 적는 대신,
/// 자격 자체에 적혀 있는 사실을 읽어 온다.
#[derive(Debug, Clone, Default)]
pub struct Probe {
    /// iam-user, oauth 등.
    pub kind: String,
    /// 로그인 이름 · ARN 등 이 계정에서 나를 가리키는 것.
    pub name: String,
    /// 계정 이름으로 쓸 만한 슬러그.
    pub slug: String,
    /// 목록에 보여줄 한 줄.
    pub display: String,
    /// `YYYY-MM-DD` 또는 `never`. 알아내지 못했으면 None.
    pub expires: Option<String>,
    /// 이 자격이 가진 권한. GitHub 토큰의 scope 등.
    pub scopes: Vec<String>,
    /// 이 계정으로 커밋할 때 쓸 이메일.
    pub git_email: Option<String>,
    /// AWS 계정 번호. 같은 계정에 속한 신원끼리 묶어 보기 위한 것이다.
    pub aws_account_id: Option<String>,
    /// root 에 액세스 키가 있는가. 읽지 못했으면 None.
    pub root_keys_present: Option<bool>,
    /// root 에 MFA 가 걸려 있는가. 읽지 못했으면 None.
    pub root_mfa: Option<bool>,
}

/// 입력한 자격으로 임시 로그인해 신원을 읽어 온다.
///
/// 임시 CLI 홈에서만 동작하므로 기존 로그인도, 아직 없는 계정도 건드리지 않는다.
pub fn probe(provider: Provider, values: &Values) -> io::Result<Probe> {
    validate(provider, values).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let scratch = home::Scratch::new(&format!("probe-{}", provider.id()))?;
    let outcome = connect_into(provider, scratch.path(), values, |_, _| {})?;
    if !outcome.ok() {
        return Err(io::Error::other("자격으로 로그인하지 못했습니다"));
    }

    match provider {
        Provider::Github => probe_github(scratch.path()),
        Provider::Aws => probe_aws(scratch.path()),
        Provider::Gcloud => probe_gcloud(scratch.path()),
        Provider::Firebase => probe_firebase(scratch.path()),
    }
}

/// 브라우저 로그인 결과를 잠시 두는 자리.
///
/// 로그인을 두 번 시키지 않기 위해서다. 확인 단계에서 여기에 로그인해 두고,
/// 계정을 만들 때 이 디렉토리를 그대로 계정 홈으로 옮긴다.
fn staging(provider: Provider) -> PathBuf {
    home::root()
        .join(home::TMP)
        .join(format!("staged-{}", provider.id()))
}

/// staging 을 비우고 새로 만든다.
fn fresh_stage(provider: Provider) -> io::Result<PathBuf> {
    let stage = staging(provider);
    // 지난 시도가 남아 있을 수 있다. 섞이지 않게 비우고 시작한다.
    let _ = std::fs::remove_dir_all(&stage);
    home::create_private(&stage)?;
    Ok(stage)
}

/// 브라우저로 로그인시키고 신원을 읽는다. 한 번에 끝나는 provider 용.
///
/// 기존 로그인도, 아직 없는 계정도 건드리지 않는다. 결과는 staging 에 남겨 두고
/// 계정을 만들 때 옮겨 쓴다.
pub fn browser_probe<F>(provider: Provider, on_line: F) -> io::Result<Probe>
where
    F: Fn(exec::Stream, String) + Send + Sync + 'static,
{
    let stage = fresh_stage(provider)?;

    let outcome = browser_login(provider, &stage, on_line)?;
    if !outcome.ok() {
        let _ = std::fs::remove_dir_all(&stage);
        return Err(io::Error::other("브라우저 로그인이 완료되지 않았습니다"));
    }

    match probe_home(provider, &stage) {
        Ok(probe) => Ok(probe),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&stage);
            Err(e)
        }
    }
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

/// 로그인을 시작해 인증 주소를 받아 온다.
pub fn browser_begin<F>(provider: Provider, on_line: F) -> io::Result<Challenge>
where
    F: Fn(exec::Stream, String) + Send + Sync + 'static,
{
    let stage = fresh_stage(provider)?;

    let program = tools::find_in_path("firebase")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "firebase 를 찾을 수 없습니다"))?;

    let buffer = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = buffer.clone();

    exec::run_env(
        &program,
        &["login", "--no-localhost"],
        &env_for(provider, &stage),
        move |stream, line| {
            if let Ok(mut buf) = sink.lock() {
                buf.push_str(&line);
                buf.push('\n');
            }
            on_line(stream, line);
        },
    )?;

    let note = buffer.lock().map(|b| b.clone()).unwrap_or_default();
    let url = first_url(&note).ok_or_else(|| {
        let _ = std::fs::remove_dir_all(&stage);
        io::Error::other("인증 주소를 찾지 못했습니다")
    })?;

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
pub fn browser_complete<F>(provider: Provider, code: &str, on_line: F) -> io::Result<Probe>
where
    F: Fn(exec::Stream, String) + Send + Sync + 'static,
{
    let code = code.trim();
    if code.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "코드를 입력하세요",
        ));
    }

    let stage = staging(provider);
    if !stage.is_dir() {
        return Err(io::Error::other("로그인을 먼저 시작하세요"));
    }

    // firebase 는 코드 시도가 한 번 실패하면 세션 상태를 지운다. 그 뒤로는 어떤
    // 코드를 넣어도 같은 오류가 나는데, 메시지가 "코드가 틀렸다" 로만 보여
    // 원인을 알 수 없다. 남아 있는지 먼저 보고 아니면 그렇다고 말한다.
    if !has_pending_login(&stage) {
        return Err(io::Error::other(
            "이 로그인 세션은 이미 끝났습니다. 코드를 한 번 잘못 넣으면 세션이 소멸하므로 다시 시작해 새 주소와 코드를 받으세요",
        ));
    }

    let program = tools::find_in_path("firebase")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "firebase 를 찾을 수 없습니다"))?;

    // 첫 단계와 같은 설정 홈이어야 한다. 세션과 검증자가 거기 들어 있다.
    let buffer = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = buffer.clone();

    let outcome = exec::run_env(
        &program,
        &["login", code],
        &env_for(provider, &stage),
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
    probe_home(provider, &stage)
}

/// 코드를 기다리는 로그인 세션이 남아 있는가.
///
/// firebase 는 세션과 검증자를 configstore 에 `tempLoginState` 로 둔다.
/// 코드 교환을 시도하면 성공이든 실패든 지운다.
fn has_pending_login(stage: &std::path::Path) -> bool {
    std::fs::read_to_string(stage.join("configstore").join("firebase-tools.json"))
        .map(|text| text.contains("tempLoginState"))
        .unwrap_or(false)
}

/// 출력에서 첫 번째 https 주소를 뽑는다.
fn first_url(text: &str) -> Option<String> {
    let start = text.find("https://")?;
    let rest = &text[start..];
    // 공백이나 줄바꿈에서 끊는다. CLI 가 주소 뒤에 안내를 붙이는 경우가 있다.
    let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// 확인 단계에서 로그인해 둔 것을 계정 홈으로 옮긴다.
///
/// 옮길 게 없으면 false 를 돌려 호출자가 다시 로그인시키게 한다.
fn adopt_staged(provider: Provider, home_dir: &std::path::Path) -> io::Result<bool> {
    let stage = staging(provider);
    if !stage.is_dir() {
        return Ok(false);
    }

    // 계정 홈은 save() 가 미리 만들어 두므로 비어 있다. 자리를 비우고 옮긴다.
    if home_dir.exists() {
        std::fs::remove_dir_all(home_dir)?;
    }
    if let Some(parent) = home_dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&stage, home_dir)?;
    home::restrict(home_dir)?;
    Ok(true)
}

/// 이미 로그인된 홈에서 신원을 읽는다.
///
/// 브라우저 로그인은 확인 단계가 따로 없다 — 로그인 자체가 확인이므로,
/// 로그인이 끝난 뒤 그 홈을 그대로 읽는다.
pub fn probe_home(provider: Provider, home_dir: &std::path::Path) -> io::Result<Probe> {
    probe_home_logging(provider, home_dir, |_, _| {})
}

/// 신원을 읽으면서 실행 명령을 터미널에도 보여 준다.
pub fn probe_home_logging<F>(
    provider: Provider,
    home_dir: &std::path::Path,
    on_line: F,
) -> io::Result<Probe>
where
    F: Fn(exec::Stream, String) + Send + Sync + 'static,
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

fn probe_gcloud(home_dir: &std::path::Path) -> io::Result<Probe> {
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
    let account = fields
        .first()
        .copied()
        .unwrap_or_default()
        .trim()
        .to_string();
    if !outcome.ok() || account.is_empty() {
        return Err(io::Error::other("Google 계정을 읽지 못했습니다"));
    }

    let project = fields.get(1).copied().unwrap_or_default().trim();

    Ok(Probe {
        kind: "oauth".into(),
        name: account.clone(),
        slug: slugify(&account),
        display: if project.is_empty() {
            "Google Cloud".to_string()
        } else {
            format!("Google Cloud · {project}")
        },
        git_email: None,
        aws_account_id: None,
        root_keys_present: None,
        root_mfa: None,
        // OAuth 자격은 갱신 토큰으로 이어지므로 만료를 우리가 셀 수 없다.
        expires: Some(crate::account::NEVER.to_string()),
        scopes: Vec::new(),
    })
}

fn probe_firebase(home_dir: &std::path::Path) -> io::Result<Probe> {
    let (outcome, raw) = capture(Provider::Firebase, home_dir, "firebase", &["login:list"])?;

    // `Logged in as tuk@tuk.im` 형태로 온다. 안내 전문이 아니라 주소만 남긴다.
    // --json 은 토큰까지 담아 오므로 쓰지 않는다.
    let account = raw
        .split_whitespace()
        .find(|token| token.contains('@'))
        .unwrap_or_default()
        .to_string();

    if !outcome.ok() || account.is_empty() {
        return Err(io::Error::other("Firebase 계정을 읽지 못했습니다"));
    }

    Ok(Probe {
        kind: "oauth".into(),
        name: account.clone(),
        slug: slugify(&account),
        display: "Firebase".to_string(),
        git_email: None,
        aws_account_id: None,
        root_keys_present: None,
        root_mfa: None,
        expires: Some(crate::account::NEVER.to_string()),
        scopes: Vec::new(),
    })
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

/// 응답 헤더 한 줄에서 값을 꺼낸다. 헤더 이름은 대소문자를 가리지 않는다.
fn header<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let name = name.to_ascii_lowercase();
    text.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.trim().to_ascii_lowercase() == name).then(|| value.trim())
    })
}

fn probe_github(home_dir: &std::path::Path) -> io::Result<Probe> {
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

    let id = fields.get(1).copied().unwrap_or_default();
    let public_email = fields.get(2).copied().unwrap_or_default().trim();

    // 공개 이메일이 없으면 GitHub 이 주는 noreply 주소를 쓴다.
    // 커밋이 계정에 붙으면서 실제 주소는 드러나지 않는다.
    let git_email = if public_email.is_empty() {
        (!id.is_empty()).then(|| format!("{id}+{login}@users.noreply.github.com"))
    } else {
        Some(public_email.to_string())
    };

    // 헤더에 토큰의 만료일과 scope 가 실려 온다. 사람이 적을 필요가 없다.
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

    Ok(Probe {
        kind: "oauth".into(),
        name: login.clone(),
        slug: slugify(&login),
        display: login,
        git_email,
        aws_account_id: None,
        root_keys_present: None,
        root_mfa: None,
        expires,
        scopes: header(&headers, "x-oauth-scopes")
            .map(|raw| {
                raw.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn probe_aws(home_dir: &std::path::Path) -> io::Result<Probe> {
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

    let (account_id, user) = parse_arn(&arn);
    if user.is_empty() {
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
                "{user} 는 IAM 계정 정보를 읽을 수 없습니다. 마스터 계정은 관리자 권한이 필요합니다"
            ),
        ));
    };

    // `0\t1` 형태로 온다.
    let flags: Vec<&str> = summary.split_whitespace().collect();
    let flag = |i: usize| flags.get(i).map(|v| *v == "1");

    let account_label = alias.unwrap_or_else(|| account_id.clone());

    Ok(Probe {
        kind: "iam-user".into(),
        name: arn,
        // 계정 번호가 아니라 IAM 사용자를 이름으로 쓴다. 한 AWS 계정에 사용자가
        // 여럿이면 번호로는 서로 구분되지 않는다.
        slug: slugify(&user),
        display: format!("AWS {account_label}"),
        git_email: None,
        aws_account_id: (!account_id.is_empty()).then_some(account_id),
        root_keys_present: flag(0),
        root_mfa: flag(1),
        // 액세스 키에는 기한이 없다. 회전은 정책으로 한다.
        expires: Some(crate::account::NEVER.to_string()),
        scopes: Vec::new(),
    })
}

/// ARN 에서 계정 번호와 신원 이름을 뽑는다.
///
/// `arn:aws:iam::123456789012:user/david` 가 기본이고,
/// 역할을 맡은 경우 `arn:aws:sts::123456789012:assumed-role/Role/session` 로 온다.
fn parse_arn(arn: &str) -> (String, String) {
    let fields: Vec<&str> = arn.split(':').collect();
    let account_id = fields.get(4).copied().unwrap_or_default().to_string();

    // 마지막 조각이 신원 이름이다. assumed-role 은 세션 이름이 맨 뒤에 온다.
    let resource = fields.get(5).copied().unwrap_or_default();
    let name = if resource.starts_with("assumed-role/") {
        resource.split('/').nth(1).unwrap_or_default()
    } else {
        resource.rsplit('/').next().unwrap_or_default()
    };

    (account_id, name.to_string())
}

/// 사람이 읽는 이름을 슬러그 규칙에 맞게 다듬는다.
fn slugify(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').chars().take(48).collect()
}

/// 새 자격이 같은 계정의 것인가.
///
/// 아니면 `david-lee-dev` 라는 이름 아래 엉뚱한 계정이 들어앉고, 격리 홈까지
/// 덮어써서 나중에 알아챌 방법이 없다. 그래서 붙이기 전에 막는다.
///
/// 기존 신원을 모르는 경우(아직 검증 전)는 비교할 대상이 없으므로 통과시킨다.
pub fn same_account(expected: &str, actual: &str) -> Result<(), String> {
    if expected.is_empty() || expected == actual {
        return Ok(());
    }
    Err(format!(
        "다른 계정의 자격입니다. 이 계정은 {expected} 인데 넣은 자격은 {actual} 입니다"
    ))
}

/// 같은 계정의 자격만 바꾼다.
///
/// 토큰은 기한을 늘릴 수 없으므로, 만료가 다가오면 GitHub 에서 재발급받아
/// 새 값을 넣는 수밖에 없다. 계정 자체는 그대로 두고 자격만 갈아 끼운다.
///
/// 새 자격이 **다른 계정의 것이면 거부한다.** 그대로 받아들이면 `david-lee-dev`
/// 라는 이름 아래 엉뚱한 계정이 들어앉고, 나중에 알아챌 방법이 없다.
pub fn replace<F>(account: &Account, values: &Values, on_line: F) -> io::Result<Probe>
where
    F: Fn(exec::Stream, String) + Send + Sync + 'static,
{
    // 붙이기 전에 누구 자격인지부터 본다. 임시 홈에서 확인하므로
    // 실패해도 지금 쓰고 있는 자격은 멀쩡하다.
    let probe = probe(account.provider, values)?;

    if let Err(message) = same_account(&account.identity.name, &probe.name) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, message));
    }

    connect_into(account.provider, &account.cli_home(), values, on_line)?;
    Ok(probe)
}
