//! 계정 기록·교체 이력·아카이브의 실제 파일.
//!
//! 계정이 보인다는 것은 그 자격이 제자리에 있다는 뜻이어야 한다. 그래서 계정 기록
//! 파일은 언제나 **마지막에** 쓴다.

use std::io;
use std::path::PathBuf;

use secrets_core::account::{Account, ArchiveReason, Provider, Replacement};

use crate::vault::paths::{self, FILE, HISTORY};
use crate::clock;
use crate::vault;

/// 번들 디렉토리와 CLI 홈을 만들고 `account.toml` 을 쓴다.
///
/// 기록은 **옆에 쓰고 제자리로 옮긴다.** 있던 파일에 바로 쓰면 도중에 실패했을 때
/// 이전 내용이 이미 잘려 나간 뒤다 — 오류를 올려 봐야 되돌릴 것이 없다.
pub fn save(account: &Account) -> io::Result<()> {
    let dir = paths::dir(account);
    vault::create_private(&dir)?;
    vault::create_private(&paths::cli_home(account))?;

    let text = toml::to_string_pretty(account)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    write_atomically(&dir.join(FILE), text.as_bytes())
}

/// 임시 파일에 다 쓴 뒤 제자리로 옮긴다. 같은 디렉토리 안이라 rename 이 원자적이다.
fn write_atomically(path: &std::path::Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;

    let staging = path.with_extension("writing");
    {
        let mut file = std::fs::File::create(&staging)?;
        file.write_all(bytes)?;
        // 내용이 디스크에 닿기 전에 rename 되면 빈 파일이 제자리에 남는다.
        file.sync_all()?;
    }
    vault::restrict(&staging)?;

    if let Err(e) = std::fs::rename(&staging, path) {
        let _ = std::fs::remove_file(&staging);
        return Err(e);
    }
    Ok(())
}

pub fn exists(provider: Provider, slug: &str) -> bool {
    paths::dir_of(provider, slug).join(FILE).is_file()
}

pub fn load(provider: Provider, slug: &str) -> io::Result<Account> {
    let text = std::fs::read_to_string(paths::dir_of(provider, slug).join(FILE))?;
    toml::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// 등록된 계정 전부. 읽을 수 없는 항목은 건너뛰지 않고 오류로 남긴다.
pub fn list() -> Vec<Result<Account, String>> {
    let mut found = Vec::new();
    let root = vault::root().join(vault::ACCOUNTS);

    for provider in Provider::ALL {
        let dir = root.join(provider.id());
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };

        let mut slugs: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        slugs.sort();

        for slug in slugs {
            if !exists(*provider, &slug) {
                continue;
            }
            found.push(
                load(*provider, &slug)
                    .map_err(|e| format!("{}/{slug}를 읽을 수 없습니다: {e}", provider.id())),
            );
        }
    }
    found
}

/// 교체 이력이 쌓일 자리. 같은 날 두 번 교체해도 덮어쓰지 않는다.
pub fn history_dir(account: &Account, day: &str) -> PathBuf {
    unique(paths::dir(account).join(HISTORY).join(day))
}

/// 교체 기록을 남긴다. 자격의 값은 담지 않는다.
pub fn write_history(dir: &std::path::Path, record: &Replacement) -> io::Result<()> {
    vault::create_private(dir)?;

    let text = toml::to_string_pretty(record)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    write_atomically(&dir.join("replaced.toml"), text.as_bytes())
}

/// 지난 교체 기록. 최근 것이 앞에 온다.
pub fn history(account: &Account) -> Vec<Replacement> {
    let Ok(entries) = std::fs::read_dir(paths::dir(account).join(HISTORY)) else {
        return Vec::new();
    };

    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs.reverse();

    dirs.iter()
        .filter_map(|d| std::fs::read_to_string(d.join("replaced.toml")).ok())
        .filter_map(|t| toml::from_str(&t).ok())
        .collect()
}

/// 계정 전체를 아카이브로 물린다. 실물을 지우지 않는다.
///
/// 지우는 대신 옮기는 이유는, 자격이 이미 죽었더라도 "무엇을 언제 썼는지" 는
/// 남아야 하기 때문이다. 교체 이력도 계정 디렉토리에 들어 있어 함께 따라간다.
pub fn archive_account(
    provider: Provider,
    slug: &str,
    reason: ArchiveReason,
) -> io::Result<PathBuf> {
    let source = paths::dir_of(provider, slug);
    if !source.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{}/{slug}을(를) 찾을 수 없습니다.", provider.id()),
        ));
    }

    // 무엇을 왜 물렸는지는 **옮기기 전에** 써 둔다. 옮긴 뒤에 쓰면 그 쓰기가
    // 실패했을 때 계정은 이미 사라졌는데 실패를 돌려주게 된다.
    let note = format!(
        "archived_at = \"{}\"\nreason = \"{}\"\nprovider = \"{}\"\nslug = \"{slug}\"\n",
        clock::now(),
        reason.id(),
        provider.id(),
    );
    let marker = source.join("archived.toml");
    write_atomically(&marker, note.as_bytes())?;

    let target = unique(
        vault::root()
            .join("archive")
            .join(vault::ACCOUNTS)
            .join(provider.id())
            .join(format!("{slug}-{}", clock::today())),
    );
    if let Some(parent) = target.parent() {
        vault::create_private(parent)?;
    }

    if let Err(e) = std::fs::rename(&source, &target) {
        // 옮기지 못했으면 계정은 제자리에 그대로 있어야 한다.
        let _ = std::fs::remove_file(&marker);
        return Err(e);
    }
    Ok(target)
}

/// 같은 이름이 있으면 뒤에 번호를 붙인다.
fn unique(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("item");
    for n in 2..100 {
        let candidate = path.with_file_name(format!("{name}-{n}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    path
}
