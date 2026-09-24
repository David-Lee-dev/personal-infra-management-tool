//! AWS pem 키가 이 머신에서 사는 방식.
//!
//! ```text
//! keys/aws/<계정ID>/<ec2|lightsail>/<리전>/<키페어>/
//!   key        0600   개인 키 (pem)
//!   key.toml          기록
//! ```
//!
//! 들이기는 **지문이 맞을 때만** 한다. AWS 는 개인 키를 다시 주지 않으므로, 엉뚱한
//! 키를 들이면 그 사실을 나중에 알아차릴 방법이 없다. 맞는지 확인하는 것이 이 모듈이
//! 하는 일의 전부라 해도 된다.

use std::io;
use std::path::{Path, PathBuf};

use secrets_core::aws::{AwsError, KeyPairRecord, fingerprint};

use crate::clock;
use crate::vault;

pub const FILE: &str = "key.toml";
pub const PRIVATE: &str = "key";

fn storage(e: impl std::fmt::Display) -> AwsError {
    AwsError::Storage(e.to_string())
}

pub fn root() -> PathBuf {
    vault::root().join(crate::keys::paths::KEYS).join("aws")
}

pub fn dir_of(account: &str, machine: &str, region: &str, name: &str) -> PathBuf {
    root().join(account).join(machine).join(region).join(name)
}

fn dir(record: &KeyPairRecord) -> PathBuf {
    dir_of(&record.account, &record.machine, &record.region, &record.name)
}

/// 개인 키를 금고로 들인다. **옮긴다** — 있던 자리에서는 지운다.
///
/// 사본을 남기면 흩어진 상태가 그대로 남는다. 이 도구가 없애려던 것이 그것이다.
///
/// `expected` 는 AWS 가 말하는 지문이다. 주어지면 맞아야 들이고, `None` 이면
/// 확인하지 못한 것으로 기록한다 — 확인한 것과 같은 척하지 않는다.
///
/// 원본을 지우는 것은 **금고에 들어간 것이 온전한지 다시 확인한 뒤**다. 하나뿐인
/// 다른 사본을 없애는 일이라 순서를 뒤집으면 안 된다.
pub fn adopt(
    mut record: KeyPairRecord,
    pem: &Path,
    expected: Option<&str>,
) -> Result<KeyPairRecord, AwsError> {
    let found = fingerprint_of(pem)?;
    if let Some(expected) = expected
        && !fingerprint::same(&found, expected)
    {
        return Err(AwsError::Mismatch {
            expected: expected.to_string(),
            found,
        });
    }

    let at = dir(&record);
    if at.join(FILE).is_file() {
        return Err(AwsError::Taken(record.slug()));
    }

    let material = std::fs::read(pem).map_err(storage)?;
    vault::create_private(&at).map_err(storage)?;
    write_atomically(&at.join(PRIVATE), &material).map_err(storage)?;

    record.fingerprint = found;
    record.verified = expected.is_some();
    record.adopted_at = clock::now();
    save(&record)?;

    // 금고 쪽이 읽히고 지문까지 같아야 원본을 지운다.
    let landed = fingerprint_of(&at.join(PRIVATE))?;
    if !fingerprint::same(&landed, &record.fingerprint) {
        return Err(AwsError::Storage(
            "금고에 들어간 키가 원본과 다릅니다. 원본을 지우지 않았습니다".into(),
        ));
    }
    std::fs::remove_file(pem).map_err(storage)?;
    // 공개 키가 옆에 있으면 같이 옮긴다. 혼자 남으면 무엇의 짝인지 알 수 없다.
    let _ = std::fs::remove_file(pem.with_extension("pub"));

    Ok(record)
}

pub fn save(record: &KeyPairRecord) -> Result<(), AwsError> {
    let at = dir(record);
    vault::create_private(&at).map_err(storage)?;
    let text = toml::to_string_pretty(record).map_err(storage)?;
    write_atomically(&at.join(FILE), text.as_bytes()).map_err(storage)
}

pub fn load(account: &str, machine: &str, region: &str, name: &str) -> Result<KeyPairRecord, AwsError> {
    let path = dir_of(account, machine, region, name).join(FILE);
    let text = std::fs::read_to_string(&path)
        .map_err(|_| AwsError::Storage(format!("{name} 기록을 읽지 못했습니다")))?;
    toml::from_str(&text).map_err(storage)
}

/// 금고에 있는 것 전부. 읽지 못한 항목은 건너뛰지 않고 오류로 남긴다.
pub fn list() -> Vec<Result<KeyPairRecord, String>> {
    let mut found = Vec::new();
    for account in dirs_in(&root()) {
        for machine in dirs_in(&root().join(&account)) {
            for region in dirs_in(&root().join(&account).join(&machine)) {
                let at = root().join(&account).join(&machine).join(&region);
                for name in dirs_in(&at) {
                    if !at.join(&name).join(FILE).is_file() {
                        continue;
                    }
                    found.push(
                        load(&account, &machine, &region, &name)
                            .map_err(|e| format!("{account}/{machine}/{region}/{name}: {e}")),
                    );
                }
            }
        }
    }
    found
}

/// `ssh-keygen -l` 이 말하는 지문. 우리가 계산하지 않는다.
pub fn fingerprint_of(path: &Path) -> Result<String, AwsError> {
    use crate::cli::{exec, tools};

    let program = tools::find_in_path("ssh-keygen")
        .ok_or_else(|| AwsError::Storage("ssh-keygen 을 찾을 수 없습니다".into()))?;

    let buffer = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = buffer.clone();
    let outcome = exec::run(
        &program,
        &["-l", "-f", &path.display().to_string()],
        move |stream, line| {
            if stream == exec::Stream::Stdout {
                sink.lock().unwrap().push_str(&line);
            }
        },
    )
    .map_err(storage)?;

    if !outcome.ok() {
        return Err(AwsError::Storage(format!(
            "{} 에서 키를 읽지 못했습니다",
            shorten(path)
        )));
    }

    let text = buffer.lock().unwrap().clone();
    text.split_whitespace()
        .find(|part| part.starts_with("SHA256:"))
        .map(str::to_string)
        .ok_or_else(|| AwsError::Storage("지문을 읽지 못했습니다".into()))
}

/// 이 키의 지문을 AWS 가 적는 방식마다 하나씩. 첫째는 [`fingerprint_of`] 의 것이다.
///
/// RSA 키페어에 AWS 는 공개 키 DER 의 MD5(가져온 키 · Lightsail)나 PKCS#8 DER 의
/// SHA1(EC2 가 만든 키)을 준다. `openssl` 로 같은 값을 계산한다. OpenSSH 형식 키는
/// `openssl` 이 읽지 못해 그 둘이 빠지는데, 그런 키(ED25519)에 AWS 는 SHA256 을 준다.
pub fn fingerprints_of(path: &Path) -> Result<Vec<String>, AwsError> {
    let mut all = vec![fingerprint_of(path)?];
    let derived = [
        "pkey -in \"$1\" -pubout -outform DER | \"$0\" dgst -md5 -c",
        "pkcs8 -topk8 -nocrypt -in \"$1\" -outform DER | \"$0\" dgst -sha1 -c",
    ];
    all.extend(derived.iter().filter_map(|pipe| digest_by_openssl(pipe, path)));
    Ok(all)
}

/// `openssl <pipe>` 의 다이제스트. 읽지 못하면 `None` — 그 방식의 지문이 없을 뿐이다.
fn digest_by_openssl(pipe: &str, path: &Path) -> Option<String> {
    use crate::cli::{exec, tools};

    let openssl = tools::find_in_path("openssl")?;
    let shell = tools::find_in_path("sh")?;
    // pipefail 이 없으면 앞 단이 실패해도 빈 입력의 다이제스트가 나온다.
    let script = format!("set -o pipefail; \"$0\" {pipe}");
    let buffer = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = buffer.clone();
    let outcome = exec::run(
        &shell,
        &["-c", &script, &openssl.display().to_string(), &path.display().to_string()],
        move |stream, line| {
            if stream == exec::Stream::Stdout {
                sink.lock().unwrap().push_str(&line);
            }
        },
    )
    .ok()?;
    if !outcome.ok() {
        return None;
    }
    // `MD5(stdin)= 40:85:…` — 마지막 조각이 값이다.
    let text = buffer.lock().unwrap().clone();
    text.split_whitespace().last().map(str::to_string)
}

/// 홈 아래 경로는 `~` 로 적는다. 기록에 절대경로가 박히면 홈이 바뀔 때 거짓이 된다.
fn shorten(path: &Path) -> String {
    let text = path.display().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() => match text.strip_prefix(&home) {
            Some(rest) => format!("~{rest}"),
            None => text,
        },
        _ => text,
    }
}

fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;

    let staging = path.with_extension("writing");
    {
        let mut file = std::fs::File::create(&staging)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    vault::restrict(&staging)?;

    if let Err(e) = std::fs::rename(&staging, path) {
        let _ = std::fs::remove_file(&staging);
        return Err(e);
    }
    Ok(())
}

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
