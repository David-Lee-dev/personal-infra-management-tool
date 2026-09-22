//! 이 머신의 CLI 로 마스터 계정 provider 에게 묻는 어댑터.
//!
//! 확인 중인 자격은 [`CredentialStore`] 가 쥔다. core 는 자격이 어디에 있는지
//! 알지 못하고 표만 주고받으며, 계정을 만들 때 레지스트리 어댑터가 같은 보관소에서
//! 그 자리를 넘겨받는다.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::account::{Account, Provider};
use crate::connect::{self, Values};
use crate::credential::CredentialInput;
use crate::identity::Observation;
use crate::port::{
    AccountGateway, Channel, GatewayError, LoginChallenge, PreparationId, Prepared, ProgressSink,
};
use crate::secret::Secret;
use crate::{exec, home};

/// 확인 중인 자격이 담긴 격리 홈들.
///
/// 준비마다 홈이 따로다. provider 당 한 자리를 공유하면 로그인 두 건이 겹칠 때
/// 나중 것이 앞 세션을 지운다.
#[derive(Default)]
pub struct CredentialStore {
    staged: Mutex<HashMap<String, Staged>>,
}

struct Staged {
    provider: Provider,
    home: PathBuf,
    /// 확인 때 본 신원. 화면이 돌려보낸 값을 믿지 않기 위해 여기 둔다.
    observed: Option<Observation>,
}

impl CredentialStore {
    pub fn new() -> CredentialStore {
        CredentialStore::default()
    }

    fn open(&self, provider: Provider) -> std::io::Result<(PreparationId, PathBuf)> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = format!(
            "prep-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );

        let stage = home::root().join(home::TMP).join(&id);
        let _ = std::fs::remove_dir_all(&stage);
        home::create_private(&stage)?;

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

    fn home_of(&self, id: &PreparationId) -> Option<(Provider, PathBuf)> {
        self.staged
            .lock()
            .unwrap()
            .get(id.as_str())
            .map(|s| (s.provider, s.home.clone()))
    }

    fn remember(&self, id: &PreparationId, observed: &Observation) {
        if let Some(entry) = self.staged.lock().unwrap().get_mut(id.as_str()) {
            entry.observed = Some(observed.clone());
        }
    }

    fn observed(&self, id: &PreparationId) -> Option<Observation> {
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

/// CLI 로 신원을 확인하는 게이트웨이.
pub struct CliAccounts {
    store: Arc<CredentialStore>,
}

impl CliAccounts {
    pub fn new(store: Arc<CredentialStore>) -> CliAccounts {
        CliAccounts { store }
    }
}

/// 실행 결과를 게이트웨이의 실패 이유로 옮긴다.
fn failed(e: std::io::Error) -> GatewayError {
    match e.kind() {
        std::io::ErrorKind::NotFound => GatewayError::Unavailable(e.to_string()),
        std::io::ErrorKind::PermissionDenied => GatewayError::NotPermitted(e.to_string()),
        _ => GatewayError::Rejected(e.to_string()),
    }
}

impl CliAccounts {
    fn staged_result(
        &self,
        id: PreparationId,
        observed: std::io::Result<Observation>,
    ) -> Result<Prepared, GatewayError> {
        match observed {
            Ok(observation) => {
                self.store.remember(&id, &observation);
                Ok(Prepared { id, observation })
            }
            Err(e) => {
                self.store.drop_staged(&id);
                Err(failed(e))
            }
        }
    }
}

impl AccountGateway for CliAccounts {
    fn prepare(
        &self,
        provider: Provider,
        credential: CredentialInput,
        progress: &dyn ProgressSink,
    ) -> Result<Prepared, GatewayError> {
        if !credential.provider_matches(provider) {
            return Err(GatewayError::Rejected(
                "이 provider 에 맞지 않는 자격입니다".into(),
            ));
        }

        let (id, stage) = self.store.open(provider).map_err(failed)?;
        let observed = connect::probe_in(provider, &stage, &values_of(credential), sink(progress));
        self.staged_result(id, observed)
    }

    fn prepare_with_browser(
        &self,
        provider: Provider,
        progress: &dyn ProgressSink,
    ) -> Result<Prepared, GatewayError> {
        let (id, stage) = self.store.open(provider).map_err(failed)?;
        let observed = connect::browser_probe_in(provider, &stage, sink(progress));
        self.staged_result(id, observed)
    }

    fn begin_browser_login(
        &self,
        provider: Provider,
        progress: &dyn ProgressSink,
    ) -> Result<(PreparationId, LoginChallenge), GatewayError> {
        let (id, stage) = self.store.open(provider).map_err(failed)?;

        match connect::browser_begin_in(provider, &stage, sink(progress)) {
            Ok(challenge) => Ok((
                id,
                LoginChallenge {
                    url: challenge.url,
                    session: challenge.session,
                    note: challenge.note,
                },
            )),
            Err(e) => {
                self.store.drop_staged(&id);
                Err(failed(e))
            }
        }
    }

    fn complete_browser_login(
        &self,
        id: &PreparationId,
        code: &Secret,
        progress: &dyn ProgressSink,
    ) -> Result<Prepared, GatewayError> {
        // 코드 교환은 성공이든 실패든 세션을 소멸시키므로 시도 후 준비를 버린다.
        // 그래서 여기서 못 찾는다는 건 그 로그인이 이미 끝났다는 뜻이다.
        let (provider, stage) = self.home_of_or_gone(id)?;

        let observed =
            connect::browser_complete_in(provider, &stage, code.expose(), sink(progress));
        self.staged_result(id.clone(), observed)
    }

    fn identity(
        &self,
        account: &Account,
        progress: &dyn ProgressSink,
    ) -> Result<Observation, GatewayError> {
        connect::probe_home_logging(account.provider, &account.cli_home(), sink(progress))
            .map_err(failed)
    }

    fn prepared(&self, id: &PreparationId) -> Option<Observation> {
        self.store.observed(id)
    }

    fn discard(&self, id: &PreparationId) {
        self.store.drop_staged(id);
    }
}

impl CliAccounts {
    fn home_of_or_gone(
        &self,
        id: &PreparationId,
    ) -> Result<(Provider, PathBuf), GatewayError> {
        self.store.home_of(id).ok_or(GatewayError::SessionGone)
    }
}

/// 타입이 붙은 자격을 CLI 가 받는 형태로 편다.
fn values_of(credential: CredentialInput) -> Values {
    let mut values = Values::new();
    match credential {
        CredentialInput::Github { token } => {
            values.insert("token".into(), token.expose().to_string());
        }
        CredentialInput::Aws {
            access_key_id,
            secret_access_key,
        } => {
            values.insert("access_key_id".into(), access_key_id);
            values.insert(
                "secret_access_key".into(),
                secret_access_key.expose().to_string(),
            );
        }
        CredentialInput::Browser => {}
    }
    values
}

/// 진행 상황을 포트의 채널로 옮겨 그대로 흘려보낸다.
fn sink(progress: &dyn ProgressSink) -> impl Fn(exec::Stream, String) + Sync {
    move |stream, line| {
        let channel = match stream {
            exec::Stream::Stdout => Channel::Out,
            exec::Stream::Stderr => Channel::Err,
        };
        progress.line(channel, &line);
    }
}
