//! AWS — 액세스 키로 붙는다. 로그인이 아니라 설정 파일을 직접 쓴다.
//!
//! 마스터 계정은 자격을 발급할 수 있어야 하므로, IAM 계정 정보를 읽을 수 있는지까지 본다.

use std::io;

use secrets_core::account::Provider;
use secrets_core::identity::{AccountFacts, AwsPrincipalKind, ObservedIdentity, Observation};

use super::{Field, LoginFlow, Method, Values, capture};
use crate::cli::exec;
use crate::vault;

/// 액세스 키 두 칸을 받아 적는다.
pub(super) fn method() -> Method {
    Method {
        flow: LoginFlow::Credential,
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
        browser: None,
        guidance: "관리자 권한 IAM 사용자의 액세스 키를 입력하세요. 마스터 계정은 자격을 발급할 수 있어야 하므로 권한이 한정된 사용자는 등록되지 않습니다. root 자격은 넣지 마세요 — 권한을 좁힐 수 없어 이 도구가 다루지 않습니다.",
    }
}

/// 설정 파일에 적어 두는 기본 리전.
///
/// 신원 확인에는 필요 없다 — sts 와 iam 은 전역 서비스다. 그래서 폼에서 묻지 않고
/// 리전이 필요한 명령을 위해 하나 적어만 둔다.
pub const DEFAULT_REGION: &str = "ap-northeast-2";


/// AWS 는 CLI 로그인이라는 개념이 없다. 설정 파일을 쓰는 것이 곧 연결이다.
///
/// `aws configure` 를 대화형으로 돌리는 대신 파일을 직접 쓴다. 값이 어디로
/// 가는지가 명확하고, 프롬프트 순서에 의존하지 않는다.
pub(super) fn connect_aws(home_dir: &std::path::Path, values: &Values) -> io::Result<exec::Outcome> {
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
    vault::restrict(&home.join("config"))?;

    std::fs::write(
        home.join("credentials"),
        format!(
            "[default]\naws_access_key_id = {}\naws_secret_access_key = {}\n",
            get("access_key_id"),
            get("secret_access_key")
        ),
    )?;
    vault::restrict(&home.join("credentials"))?;

    Ok(exec::Outcome { code: Some(0) })
}


/// AWS 는 신원 외에 **마스터 계정 자격이 있는지**까지 본다.
pub(super) fn probe_aws(home_dir: &std::path::Path) -> io::Result<Observation> {
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
            expires: Some(secrets_core::account::NEVER.to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{connect, connect_into};
    use crate::vault::paths;
    use secrets_core::account::Account;
    use crate::vault::store;
    use crate::vault::tests_support::with_temp_root;

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
            store::save(&account).unwrap();
            let scratch = vault::Scratch::new("replace-test").unwrap();

            connect_into(
                Provider::Aws,
                scratch.path(),
                &values(&[("access_key_id", "AKIAX"), ("secret_access_key", "s")]),
                |_, _| {},
            )
            .unwrap();

            // 계정 홈이 아니라 지정한 곳에 쓰여야 한다. 확인 단계가 이걸 쓴다.
            assert!(scratch.path().join("credentials").is_file());
            assert!(!paths::cli_home(&account).join("credentials").exists());
        });
    }


    #[test]
    fn aws_asks_only_for_the_credential() {
        let fields = super::method().fields;
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
            store::save(&account).unwrap();

            connect(
                &account,
                &values(&[("access_key_id", "AKIAX"), ("secret_access_key", "s")]),
                |_, _| {},
            )
            .unwrap();

            let config = std::fs::read_to_string(paths::cli_home(&account).join("config")).unwrap();
            assert!(config.contains(DEFAULT_REGION), "{config}");
        });
    }

    #[test]
    fn aws_writes_into_the_accounts_own_files() {
        with_temp_root(|_| {
            let account = Account::new(Provider::Aws, "tuk");
            store::save(&account).unwrap();

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

            let creds = std::fs::read_to_string(paths::cli_home(&account).join("credentials")).unwrap();
            assert!(creds.contains("AKIAEXAMPLE"));

            let config = std::fs::read_to_string(paths::cli_home(&account).join("config")).unwrap();
            assert!(config.contains("us-east-1"));

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(paths::cli_home(&account).join("credentials"))
                    .unwrap()
                    .permissions()
                    .mode();
                assert_eq!(mode & 0o777, 0o600, "시크릿 파일은 0600 이어야 한다");
            }
        });
    }

}
