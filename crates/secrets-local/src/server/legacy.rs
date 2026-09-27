//! 등록하지 않은 서버를 찾는 근거 — 서버 기록이 생기기 전의 계정 기록과 `~/.ssh/config`.
//!
//! 둘 다 읽기만 한다.
//!
//! ```text
//! keys/aws/<계정ID>/<머신>/<리전>/<키페어>/instance/<인스턴스>/<계정>/key.toml
//! ```

use std::path::Path;

use secrets_core::aws::instance::{self, InstanceAccount};
use secrets_core::server::{AccountState, ConfigHost, LegacyAccount};

use crate::{aws_vault, keys::hosts, vault};

/// 서버 계정이 놓일 수 있는 머신 종류.
const MACHINES: &[&str] = &["ec2", "lightsail"];

fn dirs_in(path: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

/// 옛 계정 기록 전부. 읽지 못한 기록은 건너뛴다 — 제안의 근거일 뿐 정본이 아니다.
pub fn legacy_accounts() -> Vec<LegacyAccount> {
    let root = aws_vault::root();
    let vault_root = vault::root();
    let mut found = Vec::new();
    for aws_account in dirs_in(&root) {
        for machine in MACHINES {
            let base = root.join(&aws_account).join(machine);
            for region in dirs_in(&base) {
                for keypair in dirs_in(&base.join(&region)) {
                    let under = base.join(&region).join(&keypair).join("instance");
                    for instance in dirs_in(&under) {
                        for login in dirs_in(&under.join(&instance)) {
                            let at = under.join(&instance).join(&login);
                            let Ok(text) = std::fs::read_to_string(at.join(aws_vault::FILE)) else {
                                continue;
                            };
                            let Ok(record) = toml::from_str::<InstanceAccount>(&text) else {
                                continue;
                            };
                            let key_path = at
                                .join(aws_vault::PRIVATE)
                                .strip_prefix(&vault_root)
                                .map(|p| p.display().to_string())
                                .unwrap_or_default();
                            found.push(legacy(&aws_account, machine, record, key_path));
                        }
                    }
                }
            }
        }
    }
    found
}

fn legacy(
    aws_account: &str,
    machine: &str,
    record: InstanceAccount,
    key_path: String,
) -> LegacyAccount {
    LegacyAccount {
        aws_account: aws_account.to_string(),
        machine: machine.to_string(),
        region: record.region,
        keypair: record.keypair,
        instance: record.instance,
        instance_name: record.instance_name,
        address: record.address,
        login: record.account,
        role: record.role,
        purpose: record.purpose,
        via: record.via,
        workspace: record.workspace,
        group: record.group,
        key_path,
        fingerprint: record.fingerprint,
        state: match record.state {
            instance::AccountState::Local => AccountState::Local,
            instance::AccountState::Installed => AccountState::Installed,
            instance::AccountState::Verified => AccountState::Verified,
        },
        verified_at: record.verified_at,
        ours: record.ours,
    }
}

/// `~/.ssh/config` 의 별칭들. `IdentityFile` 은 적힌 그대로와 푼 절대 경로를 함께 준다.
pub fn config_hosts() -> Vec<ConfigHost> {
    hosts::known()
        .into_iter()
        .map(|host| ConfigHost {
            identity: host.identity.as_ref().map(|written| {
                let absolute = super::keys::expand(written).display().to_string();
                (written.clone(), absolute)
            }),
            alias: host.alias,
            address: host.address,
            user: host.user,
            port: host.port,
            extras: host.extras,
        })
        .collect()
}
