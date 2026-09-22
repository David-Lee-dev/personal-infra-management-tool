//! 확인이 끝나 붙이기만 남은 자격을 잠시 두는 자리.
//!
//! 게이트웨이가 여기에 로그인을 만들어 두고, 레지스트리가 그 자리를 계정에게
//! 넘겨받는다. 두 어댑터가 같은 보관소를 공유하므로 core 는 경로를 한 번도 보지
//! 않고 표만 주고받는다.
//!
//! 자격이 담긴 디렉토리이므로 쓰이지 않게 된 것은 즉시 지운다.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use secrets_core::account::Provider;
use secrets_core::identity::Observation;
use secrets_core::port::PreparationId;

use crate::vault;

/// 확인 중인 자격이 담긴 격리 홈들.
///
/// 준비마다 홈이 따로다. provider 당 한 자리를 공유하면 로그인 두 건이 겹칠 때
/// 나중 것이 앞 세션을 지운다.
#[derive(Default)]
pub struct PreparationStore {
    staged: Mutex<HashMap<String, Staged>>,
}

struct Staged {
    provider: Provider,
    home: PathBuf,
    /// 확인 때 본 신원. 화면이 돌려보낸 값을 믿지 않기 위해 여기 둔다.
    observed: Option<Observation>,
}

impl PreparationStore {
    /// 보관소를 연다. 지난 실행이 남긴 준비 홈이 있으면 먼저 치운다.
    ///
    /// 준비 홈에는 로그인이 들어 있다. 앱이 꺼지면 그 표를 아는 사람이 없어지므로
    /// 다시는 쓰이지 않는데, 자격은 디스크에 그대로 남는다.
    pub fn new() -> PreparationStore {
        sweep_abandoned();
        PreparationStore::default()
    }

    pub(super) fn open(&self, provider: Provider) -> std::io::Result<(PreparationId, PathBuf)> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = format!(
            "prep-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );

        let stage = vault::root().join(vault::TMP).join(&id);
        let _ = std::fs::remove_dir_all(&stage);
        vault::create_private(&stage)?;

        self.staged.lock().unwrap().insert(
            id.clone(),
            Staged {
                provider,
                home: stage.clone(),
                observed: None,
            },
        );
        Ok((PreparationId::named(id), stage))
    }

    pub(super) fn home_of(&self, id: &PreparationId) -> Option<(Provider, PathBuf)> {
        self.staged
            .lock()
            .unwrap()
            .get(id.as_str())
            .map(|s| (s.provider, s.home.clone()))
    }

    pub(super) fn remember(&self, id: &PreparationId, observed: &Observation) {
        if let Some(entry) = self.staged.lock().unwrap().get_mut(id.as_str()) {
            entry.observed = Some(observed.clone());
        }
    }

    pub(super) fn observed(&self, id: &PreparationId) -> Option<Observation> {
        self.staged
            .lock()
            .unwrap()
            .get(id.as_str())
            .and_then(|s| s.observed.clone())
    }

    /// 준비된 자격의 자리를 넘겨받는다. 이후 보관소는 그 자격을 잊는다.
    pub fn take(&self, id: &PreparationId) -> Option<PathBuf> {
        self.staged.lock().unwrap().remove(id.as_str()).map(|s| s.home)
    }

    /// 준비된 자격을 지운다.
    pub fn drop_staged(&self, id: &PreparationId) {
        if let Some(stage) = self.take(id) {
            let _ = std::fs::remove_dir_all(stage);
        }
    }
}

/// 지난 실행이 남긴 준비 홈을 지운다.
fn sweep_abandoned() {
    let Ok(entries) = std::fs::read_dir(vault::root().join(vault::TMP)) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let looks_prepared = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("prep-"));
        if looks_prepared && path.is_dir() {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

