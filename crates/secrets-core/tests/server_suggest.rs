//! 등록하지 않은 서버를 찾아 제안하는 일. 근거는 옛 서버 계정 기록과 `~/.ssh/config` 다.

use secrets_core::server::suggest::suggest;
use secrets_core::server::{
    AccountKey, AccountOrigin, AccountState, ConfigHost, LegacyAccount, Role, Server, ServerKind,
};

const VAULT: &str = "/Users/d/.secrets";

fn legacy(
    instance: &str,
    name: &str,
    address: &str,
    login: &str,
    role: Role,
    ours: bool,
) -> LegacyAccount {
    LegacyAccount {
        aws_account: "320042238085".into(),
        machine: "ec2".into(),
        region: "ap-northeast-2".into(),
        keypair: "tuk-key".into(),
        instance: instance.into(),
        instance_name: name.into(),
        address: address.into(),
        login: login.into(),
        role,
        purpose: String::new(),
        via: "ubuntu".into(),
        workspace: "/srv".into(),
        group: "workspace".into(),
        key_path: format!(
            "keys/aws/320042238085/ec2/ap-northeast-2/tuk-key/instance/{instance}/{login}/key"
        ),
        fingerprint: "SHA256:x".into(),
        state: AccountState::Verified,
        verified_at: Some("t".into()),
        ours,
    }
}

fn host(alias: &str, address: &str, user: &str, identity: Option<&str>) -> ConfigHost {
    ConfigHost {
        alias: alias.into(),
        address: Some(address.into()),
        user: Some(user.into()),
        port: None,
        identity: identity.map(|written| {
            let absolute = written.replace('~', "/Users/d");
            (written.to_string(), absolute)
        }),
        extras: Vec::new(),
    }
}

fn logins(accounts: &[secrets_core::server::ServerAccount]) -> Vec<&str> {
    accounts.iter().map(|a| a.login.as_str()).collect()
}

#[test]
fn old_account_records_become_one_server_per_instance_with_the_pem_login_as_admin_access() {
    let found = suggest(
        &[
            legacy(
                "i-0b97",
                "tuk-api-server",
                "54.116.119.214",
                "deploy",
                Role::User,
                true,
            ),
            legacy(
                "i-0b97",
                "tuk-api-server",
                "54.116.119.214",
                "admin",
                Role::Admin,
                true,
            ),
            legacy(
                "i-0fbf",
                "dev-tuk-db-server",
                "3.37.156.50",
                "admin",
                Role::Admin,
                false,
            ),
        ],
        &[],
        &[],
        VAULT,
    );

    let names: Vec<&str> = found.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["dev-tuk-db-server", "tuk-api-server"]);

    let api = &found[1];
    assert_eq!(api.kind, ServerKind::Ec2);
    assert_eq!(api.aws.as_ref().unwrap().instance, "i-0b97");
    assert_eq!(api.admin.as_deref(), Some("ubuntu"));
    assert_eq!(logins(&api.accounts), vec!["admin", "deploy", "ubuntu"]);
    let deploy = &api.accounts[1];
    assert_eq!(
        deploy.key,
        AccountKey::Vault {
            path: "keys/aws/320042238085/ec2/ap-northeast-2/tuk-key/instance/i-0b97/deploy/key"
                .into()
        },
        "기존 키 파일을 그 자리에서 가리킨다"
    );
    assert_eq!(deploy.origin, AccountOrigin::Created);
    let ubuntu = &api.accounts[2];
    assert_eq!(
        (ubuntu.role, &ubuntu.key),
        (
            Role::Admin,
            &AccountKey::Pem {
                keypair: "tuk-key".into()
            }
        )
    );

    assert_eq!(
        found[0].accounts[0].origin,
        AccountOrigin::Installed,
        "이 도구가 만들지 않은 계정"
    );
}

#[test]
fn an_alias_for_a_known_address_joins_that_server_instead_of_making_another() {
    let found = suggest(
        &[legacy(
            "i-0b97",
            "tuk-api-server",
            "54.116.119.214",
            "deploy",
            Role::User,
            true,
        )],
        &[host(
            "tukapp-prod-ai",
            "54.116.119.214",
            "deploy",
            Some(
                "/Users/d/.secrets/keys/aws/320042238085/ec2/ap-northeast-2/tuk-key/instance/i-0b97/deploy/key",
            ),
        )],
        &[],
        VAULT,
    );

    assert_eq!(found.len(), 1);
    assert_eq!(logins(&found[0].accounts), vec!["deploy", "ubuntu"]);
    assert_eq!(
        found[0].sources,
        vec!["서버 계정 기록 1", "별칭 tukapp-prod-ai"]
    );
}

#[test]
fn servers_outside_aws_come_from_aliases_with_their_key_as_written() {
    let mut mac_admin = host(
        "nemo-mac-admin",
        "100.115.77.18",
        "nemobandeus",
        Some("~/.ssh/nemo-mac"),
    );
    mac_admin.extras = Vec::new();
    let found = suggest(
        &[],
        &[
            host("nemo", "nemo.tail25dc19.ts.net", "infra", None),
            host("nemo-deploy", "nemo.tail25dc19.ts.net", "nemo", None),
            host("nemo-admin", "nemo.tail25dc19.ts.net", "nemo", None),
            host(
                "nemo-mac",
                "100.115.77.18",
                "david",
                Some("~/.ssh/nemo-mac"),
            ),
            mac_admin,
        ],
        &[],
        VAULT,
    );

    assert_eq!(found.len(), 2);
    let nemo = &found[0];
    assert_eq!((nemo.name.as_str(), nemo.kind), ("nemo", ServerKind::Other));
    assert_eq!(logins(&nemo.accounts), vec!["infra", "nemo"]);
    assert_eq!(nemo.accounts[0].key, AccountKey::Agent);
    assert_eq!(nemo.admin, None, "sudo 여부는 별칭으로 알 수 없다");
    assert!(
        nemo.accounts
            .iter()
            .all(|a| a.role == Role::User && a.state == AccountState::Unverified)
    );

    let mac = &found[1];
    assert_eq!(
        mac.accounts[0].key,
        AccountKey::File {
            path: "~/.ssh/nemo-mac".into()
        }
    );
}

#[test]
fn a_pem_alias_tells_the_aws_account_region_and_kind_and_other_settings_are_listed_as_not_moved() {
    let mut vpn = host(
        "vpn",
        "54.116.177.2",
        "ubuntu",
        Some(
            "/Users/d/.secrets/keys/aws/320042238085/lightsail/ap-northeast-2/LightsailDefaultKeyPair/key",
        ),
    );
    vpn.extras = vec!["LocalForward 51821 127.0.0.1:51821".into()];

    let found = suggest(&[], &[vpn], &[], VAULT);

    let vpn = &found[0];
    assert_eq!(vpn.kind, ServerKind::Lightsail);
    let aws = vpn.aws.as_ref().unwrap();
    assert_eq!(
        (aws.account.as_str(), aws.region.as_str()),
        ("320042238085", "ap-northeast-2")
    );
    assert_eq!(
        vpn.accounts[0].key,
        AccountKey::Pem {
            keypair: "LightsailDefaultKeyPair".into()
        }
    );
    assert_eq!(vpn.skipped, vec!["vpn: LocalForward 51821 127.0.0.1:51821"]);
}

#[test]
fn git_remotes_and_hosts_without_a_user_are_not_servers() {
    let mut no_user = host("bare", "10.0.0.1", "x", None);
    no_user.user = None;
    let found = suggest(
        &[],
        &[
            host(
                "github.com",
                "github.com",
                "git",
                Some("~/.ssh/github/main"),
            ),
            no_user,
        ],
        &[],
        VAULT,
    );
    assert!(found.is_empty());
}

#[test]
fn a_server_already_registered_by_address_or_instance_is_not_suggested() {
    let registered: Server = toml::from_str(
        r#"
id = "tuk-api-server"
name = "tuk-api-server"
address = "9.9.9.9"
kind = "ec2"
registered_at = "t"
[aws]
account = "320042238085"
region = "ap-northeast-2"
instance = "i-0b97"
"#,
    )
    .unwrap();

    let found = suggest(
        &[legacy(
            "i-0b97",
            "tuk-api-server",
            "54.116.119.214",
            "deploy",
            Role::User,
            true,
        )],
        &[host("old", "9.9.9.9", "ubuntu", None)],
        &[registered],
        VAULT,
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn the_same_instance_id_in_another_region_is_another_server() {
    let mut elsewhere = legacy("i-0b97", "copy", "9.9.9.9", "deploy", Role::User, true);
    elsewhere.region = "us-east-1".into();
    let found = suggest(
        &[
            legacy(
                "i-0b97",
                "tuk-api-server",
                "54.116.119.214",
                "deploy",
                Role::User,
                true,
            ),
            elsewhere,
        ],
        &[],
        &[],
        VAULT,
    );
    assert_eq!(found.len(), 2);
}
