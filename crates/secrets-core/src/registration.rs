//! 계정 등록 — 자격을 먼저 확인하고, 확인된 것만 레지스트리에 들인다.
//!
//! 두 단계로 나뉜다.
//!
//! 1. [`prepare`] 계열이 **격리된 준비 홈**에 실제로 로그인하고 신원을 읽는다.
//!    이 단계는 레지스트리에 아무것도 남기지 않는다.
//! 2. [`commit`] 이 그 준비 홈을 계정 홈으로 옮기고 `account.toml` 을 쓴다.
//!
//! 준비가 실패하면 계정은 존재한 적이 없고, 확정이 실패하면 만들다 만 흔적을
//! 남기지 않는다. 계정이 있다는 것은 곧 그 자격으로 로그인이 됐다는 뜻이다.
//!
//! 신원·권한·만료일은 **준비 단계의 관찰 결과만** 쓴다. 화면이 돌려보낸 값을
//! 믿지 않는다 — 사람이 적어 넣을 수 있는 것은 설명뿐이다.

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::account::{self, Account, Provider};
use crate::connect::{self, Challenge, Probe, Values};
use crate::{date, exec, home};

/// 준비된 자격을 가리키는 표. 값 자체는 이 프로세스 밖으로 나가지 않는다.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PreparationId(String);

impl PreparationId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 화면이 돌려보낸 표를 다시 읽는다.
    pub fn named(text: &str) -> PreparationId {
        PreparationId(text.to_string())
    }
}

/// 확인이 끝나 확정만 남은 자격.
struct Preparation {
    provider: Provider,
    stage: PathBuf,
    /// 확정 시점에 계정에 적힐 관찰 결과. 브라우저 로그인은 두 단계라 나중에 채워진다.
    observed: Option<Probe>,
}

/// 사람이 적는 것. 신원에서 읽을 수 없는 것만 여기 들어온다.
pub struct Draft {
    pub slug: String,
    pub display: String,
    pub note: String,
}

fn table() -> &'static Mutex<HashMap<String, Preparation>> {
    static TABLE: OnceLock<Mutex<HashMap<String, Preparation>>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_id() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("prep-{}-{n}", std::process::id())
}

fn stage_for(id: &str) -> PathBuf {
    home::root().join(home::TMP).join(id)
}

/// 준비 홈을 만들고 표에 자리를 잡는다.
fn open(provider: Provider) -> io::Result<(PreparationId, PathBuf)> {
    let id = next_id();
    let stage = stage_for(&id);
    let _ = std::fs::remove_dir_all(&stage);
    home::create_private(&stage)?;

    table().lock().unwrap().insert(
        id.clone(),
        Preparation {
            provider,
            stage: stage.clone(),
            observed: None,
        },
    );
    Ok((PreparationId(id), stage))
}

/// 관찰 결과를 표에 기록한다. 실패하면 준비를 통째로 버린다.
fn settle(id: &PreparationId, observed: io::Result<Probe>) -> io::Result<Probe> {
    match observed {
        Ok(probe) => {
            if let Some(entry) = table().lock().unwrap().get_mut(&id.0) {
                entry.observed = Some(probe.clone());
            }
            Ok(probe)
        }
        Err(e) => {
            discard(id);
            Err(e)
        }
    }
}

/// 입력한 자격으로 로그인해 신원을 읽는다. 값을 받아 적는 provider 용.
pub fn prepare<F>(
    provider: Provider,
    values: &Values,
    on_line: F,
) -> io::Result<(PreparationId, Probe)>
where
    F: Fn(exec::Stream, String) + Send + Sync + 'static,
{
    let (id, stage) = open(provider)?;
    let probe = settle(&id, connect::probe_in(provider, &stage, values, on_line))?;
    Ok((id, probe))
}

/// 브라우저로 로그인해 신원을 읽는다. 한 번에 끝나는 provider 용.
pub fn prepare_with_browser<F>(provider: Provider, on_line: F) -> io::Result<(PreparationId, Probe)>
where
    F: Fn(exec::Stream, String) + Send + Sync + 'static,
{
    let (id, stage) = open(provider)?;
    let probe = settle(&id, connect::browser_probe_in(provider, &stage, on_line))?;
    Ok((id, probe))
}

/// 코드를 받아 와야 끝나는 로그인의 첫 단계.
pub fn begin_browser_login<F>(
    provider: Provider,
    on_line: F,
) -> io::Result<(PreparationId, Challenge)>
where
    F: Fn(exec::Stream, String) + Send + Sync + 'static,
{
    let (id, stage) = open(provider)?;
    match connect::browser_begin_in(provider, &stage, on_line) {
        Ok(challenge) => Ok((id, challenge)),
        Err(e) => {
            discard(&id);
            Err(e)
        }
    }
}

/// 코드를 넣어 로그인을 끝낸다. 첫 단계와 같은 준비 홈에서 일어난다.
pub fn complete_browser_login<F>(
    id: &PreparationId,
    code: &str,
    on_line: F,
) -> io::Result<Probe>
where
    F: Fn(exec::Stream, String) + Send + Sync + 'static,
{
    let (provider, stage) = {
        let table = table().lock().unwrap();
        // 코드 교환은 성공이든 실패든 세션을 소멸시키므로 시도 후 준비를 버린다.
        // 그래서 여기서 못 찾는다는 건 "그 로그인은 이미 끝났다"는 뜻이다.
        let entry = table.get(&id.0).ok_or_else(|| {
            io::Error::other(
                "이 로그인 세션은 이미 끝났습니다. 코드를 한 번 잘못 넣으면 세션이 소멸하므로 다시 시작해 새 주소와 코드를 받으세요",
            )
        })?;
        (entry.provider, entry.stage.clone())
    };

    settle(
        id,
        connect::browser_complete_in(provider, &stage, code, on_line),
    )
}

/// 준비한 자격을 버린다. 준비 홈까지 지운다.
pub fn discard(id: &PreparationId) {
    if let Some(entry) = table().lock().unwrap().remove(&id.0) {
        let _ = std::fs::remove_dir_all(&entry.stage);
    }
}

/// 준비한 자격을 계정으로 확정한다.
///
/// 준비 홈이 계정의 CLI 홈이 되므로 같은 자격으로 다시 로그인하지 않는다.
/// `account.toml` 은 **가장 마지막에** 쓴다 — 레지스트리에 계정이 보인다는 것은
/// 자격이 이미 제자리에 있다는 뜻이어야 한다. 중간에 실패하면 만들던 것을 지운다.
pub fn commit(id: &PreparationId, draft: Draft) -> io::Result<Account> {
    account::validate_slug(&draft.slug).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let (provider, stage, observed) = {
        let mut table = table().lock().unwrap();
        let entry = table
            .remove(&id.0)
            .ok_or_else(|| io::Error::other("확인된 자격이 없습니다. 자격 확인을 먼저 하세요"))?;
        let observed = entry.observed.ok_or_else(|| {
            let _ = std::fs::remove_dir_all(&entry.stage);
            io::Error::other("확인되지 않은 자격은 등록할 수 없습니다")
        })?;
        (entry.provider, entry.stage, observed)
    };

    if account::exists(provider, &draft.slug) {
        let _ = std::fs::remove_dir_all(&stage);
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{}/{} 는 이미 있습니다", provider.id(), draft.slug),
        ));
    }

    let account = from_observation(provider, &draft, &observed);
    match place(&account, &stage) {
        Ok(()) => Ok(account),
        Err(e) => {
            let _ = std::fs::remove_dir_all(account.dir());
            let _ = std::fs::remove_dir_all(&stage);
            Err(e)
        }
    }
}

/// 관찰 결과와 사람이 적은 설명으로 계정을 만든다.
///
/// 신원·권한·만료일은 관찰 결과에서만 온다.
fn from_observation(provider: Provider, draft: &Draft, observed: &Probe) -> Account {
    let mut account = Account::new(provider, &draft.slug);
    account.display = draft.display.clone();
    account.note = draft.note.clone();
    account.identity.kind = observed.kind.clone();
    account.identity.name = observed.name.clone();
    account.scopes = observed.scopes.clone();
    account.git_email = observed.git_email.clone();
    account.aws_account_id = observed.aws_account_id.clone();
    account.root_keys_present = observed.root_keys_present;
    account.root_mfa = observed.root_mfa;
    account.expires = observed.expires.clone();
    account.verification = Some(account::Verification {
        checked_at: date::now(),
        ok: true,
        detail: observed.display.clone(),
    });
    account
}

/// 준비 홈을 계정 자리로 옮기고 마지막에 레지스트리 기록을 쓴다.
fn place(account: &Account, stage: &std::path::Path) -> io::Result<()> {
    let dir = account.dir();
    home::create_private(&dir)?;

    let cli = account.cli_home();
    if cli.exists() {
        std::fs::remove_dir_all(&cli)?;
    }
    std::fs::rename(stage, &cli)?;
    home::restrict(&cli)?;

    account.save()
}
