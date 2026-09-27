//! 아직 등록하지 않은 서버를 찾아 제안한다. 등록은 사람이 고른 것만 한다.
//!
//! 근거는 두 가지다.
//!
//! - **서버 계정 기록** — 서버 기록이 생기기 전에 pem 아래에 남긴 계정 기록. 인스턴스별로 묶는다.
//!   그 계정들을 심을 때 들어간 계정(`via`, 보통 `ubuntu`)은 pem 키 · sudo 가 있는 관리 접속이다.
//! - **`~/.ssh/config`** — 사람이 적은 별칭. 주소 · 포트로 묶고, 위의 서버와 주소가 같으면 합친다.
//!   역할은 알 수 없으므로 사용자로 두고, 관리 접속도 정하지 않는다.
//!
//! 이미 등록한 서버(같은 주소 · 포트, 또는 같은 인스턴스)는 제안하지 않는다.

use super::{
    AccountKey, AccountOrigin, AccountState, AwsFacts, Role, Server, ServerAccount, ServerKind,
};

/// 서버 기록이 생기기 전의 계정 기록 하나.
#[derive(Debug, Clone)]
pub struct LegacyAccount {
    pub aws_account: String,
    /// ec2 | lightsail
    pub machine: String,
    pub region: String,
    pub keypair: String,
    pub instance: String,
    pub instance_name: String,
    pub address: String,
    pub login: String,
    pub role: Role,
    pub purpose: String,
    /// 이 계정을 심을 때 pem 으로 들어간 계정.
    pub via: String,
    pub workspace: String,
    pub group: String,
    /// 개인 키의 자리. 시크릿 저장소 뿌리 기준 상대 경로.
    pub key_path: String,
    pub fingerprint: String,
    pub state: AccountState,
    pub verified_at: Option<String>,
    /// 이 도구가 계정까지 만들었는가.
    pub ours: bool,
}

/// `~/.ssh/config` 의 별칭 하나.
#[derive(Debug, Clone)]
pub struct ConfigHost {
    pub alias: String,
    pub address: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    /// `IdentityFile` — 적힌 그대로와, `~` 를 푼 절대 경로.
    pub identity: Option<(String, String)>,
    /// 옮기지 않는 설정 줄(`LocalForward …` 등).
    pub extras: Vec<String>,
}

/// 등록을 제안하는 서버 하나.
#[derive(Debug, Clone)]
pub struct Suggestion {
    pub name: String,
    pub address: String,
    pub port: u16,
    pub kind: ServerKind,
    pub aws: Option<AwsFacts>,
    pub admin: Option<String>,
    pub workspace: String,
    pub workspace_group: String,
    pub accounts: Vec<ServerAccount>,
    /// 어느 기록 · 별칭에서 왔는가.
    pub sources: Vec<String>,
    /// 옮기지 않는 설정.
    pub skipped: Vec<String>,
}

/// 찾은 근거로 제안을 만든다. `vault_root` 는 시크릿 저장소 뿌리의 절대 경로다.
pub fn suggest(
    legacy: &[LegacyAccount],
    hosts: &[ConfigHost],
    registered: &[Server],
    vault_root: &str,
) -> Vec<Suggestion> {
    let mut found = from_legacy(legacy);
    for host in hosts {
        merge_host(&mut found, host, vault_root);
    }
    found.retain(|s| !already_registered(s, registered));
    found
}

fn from_legacy(legacy: &[LegacyAccount]) -> Vec<Suggestion> {
    let mut groups: Vec<Vec<&LegacyAccount>> = Vec::new();
    for record in legacy {
        // 인스턴스 ID 는 AWS 계정 · 종류 · 리전 안에서만 하나다.
        let same = |g: &LegacyAccount| {
            (&g.aws_account, &g.machine, &g.region, &g.instance)
                == (
                    &record.aws_account,
                    &record.machine,
                    &record.region,
                    &record.instance,
                )
        };
        match groups.iter_mut().find(|g| same(g[0])) {
            Some(group) => group.push(record),
            None => groups.push(vec![record]),
        }
    }

    let mut found: Vec<Suggestion> = groups
        .into_iter()
        .map(|mut group| {
            group.sort_by(|a, b| a.login.cmp(&b.login));
            let first = group[0];
            let mut accounts: Vec<ServerAccount> = group
                .iter()
                .map(|r| ServerAccount {
                    login: r.login.clone(),
                    role: r.role,
                    purpose: r.purpose.clone(),
                    key: AccountKey::Vault {
                        path: r.key_path.clone(),
                    },
                    origin: if r.ours {
                        AccountOrigin::Created
                    } else {
                        AccountOrigin::Installed
                    },
                    state: r.state,
                    verified_at: r.verified_at.clone(),
                    fingerprint: r.fingerprint.clone(),
                })
                .collect();
            let admin = (!first.via.is_empty()).then(|| first.via.clone());
            if let Some(via) = &admin
                && !accounts.iter().any(|a| &a.login == via)
            {
                accounts.push(ServerAccount {
                    login: via.clone(),
                    role: Role::Admin,
                    purpose: String::new(),
                    key: AccountKey::Pem {
                        keypair: first.keypair.clone(),
                    },
                    origin: AccountOrigin::Registered,
                    state: AccountState::Unverified,
                    verified_at: None,
                    fingerprint: String::new(),
                });
            }
            Suggestion {
                name: if first.instance_name.trim().is_empty() {
                    first.instance.clone()
                } else {
                    first.instance_name.clone()
                },
                address: first.address.clone(),
                port: 22,
                kind: machine_kind(&first.machine),
                aws: Some(AwsFacts {
                    account: first.aws_account.clone(),
                    region: first.region.clone(),
                    instance: first.instance.clone(),
                }),
                admin,
                workspace: first.workspace.clone(),
                workspace_group: first.group.clone(),
                accounts,
                sources: vec![format!("서버 계정 기록 {}", group.len())],
                skipped: Vec::new(),
            }
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

fn merge_host(found: &mut Vec<Suggestion>, host: &ConfigHost, vault_root: &str) {
    // User 가 없으면 누구로 들어가는지 모른다. git 은 로그인 서버가 아니라 Git 원격이다.
    let Some(login) = host
        .user
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
    else {
        return;
    };
    if login == "git" || super::check_login(login).is_err() {
        return;
    }
    let address = host.address.clone().unwrap_or_else(|| host.alias.clone());
    if super::check_address(&address).is_err() {
        return;
    }
    let port = host.port.unwrap_or(22);
    let (key, pem_facts) = classify(host.identity.as_ref(), vault_root);

    let at = match found
        .iter()
        .position(|s| s.address == address && s.port == port)
    {
        Some(at) => at,
        None => {
            let (kind, aws) = match &pem_facts {
                Some((kind, facts)) => (*kind, Some(facts.clone())),
                None => (ServerKind::Other, None),
            };
            found.push(Suggestion {
                name: host.alias.clone(),
                address: address.clone(),
                port,
                kind,
                aws,
                admin: None,
                workspace: "/srv".into(),
                workspace_group: "workspace".into(),
                accounts: Vec::new(),
                sources: Vec::new(),
                skipped: Vec::new(),
            });
            found.len() - 1
        }
    };
    let suggestion = &mut found[at];
    suggestion.sources.push(format!("별칭 {}", host.alias));
    suggestion
        .skipped
        .extend(host.extras.iter().map(|e| format!("{}: {e}", host.alias)));

    let known = suggestion.accounts.iter().any(|a| a.login == login);
    if !known {
        suggestion.accounts.push(ServerAccount {
            login: login.to_string(),
            role: Role::User,
            purpose: String::new(),
            key,
            origin: AccountOrigin::Registered,
            state: AccountState::Unverified,
            verified_at: None,
            fingerprint: String::new(),
        });
    }
}

/// `IdentityFile` 이 무엇을 가리키는가. 시크릿 저장소의 pem 이면 그 AWS 계정 · 리전도 알 수 있다.
///
/// pem 의 자리: `keys/aws/<계정>/<머신>/<리전>/<키페어>/key`.
fn classify(
    identity: Option<&(String, String)>,
    vault_root: &str,
) -> (AccountKey, Option<(ServerKind, AwsFacts)>) {
    let Some((written, absolute)) = identity else {
        return (AccountKey::Agent, None);
    };
    let root = format!("{}/", vault_root.trim_end_matches('/'));
    let Some(relative) = absolute.strip_prefix(&root) else {
        return (
            AccountKey::File {
                path: written.clone(),
            },
            None,
        );
    };
    let parts: Vec<&str> = relative.split('/').collect();
    if let ["keys", "aws", account, machine, region, keypair, "key"] = parts.as_slice() {
        return (
            AccountKey::Pem {
                keypair: keypair.to_string(),
            },
            Some((
                machine_kind(machine),
                AwsFacts {
                    account: account.to_string(),
                    region: region.to_string(),
                    instance: String::new(),
                },
            )),
        );
    }
    (
        AccountKey::Vault {
            path: relative.to_string(),
        },
        None,
    )
}

fn machine_kind(machine: &str) -> ServerKind {
    match machine {
        "lightsail" => ServerKind::Lightsail,
        _ => ServerKind::Ec2,
    }
}

fn already_registered(found: &Suggestion, registered: &[Server]) -> bool {
    let instance = found.aws.as_ref().filter(|a| !a.instance.is_empty());
    registered.iter().any(|s| {
        (s.address == found.address && s.port == found.port)
            || instance.is_some_and(|mine| s.kind == found.kind && s.aws.as_ref() == Some(mine))
    })
}
