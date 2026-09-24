//! 키를 만들고 회전시키고 걷어내는 절차.
//!
//! 원격이 끼어들어 한 번의 `rename` 으로 묶이지 않으므로, 각 단계 사이에서
//! 죽어도 **아무것도 끊기지 않는 자리**에만 멈추도록 순서를 정한다.
//!
//! - 만들기: 로컬 먼저, 등록 나중. 등록이 실패하면 쓸 수 없는 키가 남지만
//!   `Local` 로 드러나고 재시도할 수 있다. 반대로 하면 개인 키 없는 등록이 남는다.
//! - 재발급: 새 키를 **먼저 등록**하고 옛 키를 나중에 지운다. 중간에 죽으면 둘 다
//!   살아 있어 접속이 끊기지 않는다. 반대로 하면 그 사이에 아무 키도 없다.

use crate::credential::secret::Secret;
use crate::key::{DeployKey, KeyError, KeyGateway, KeyRef, KeyState, KeyVault, RepoRef};
use crate::port::{Clock, ProgressSink};

pub struct Keyring<'a> {
    gateway: &'a dyn KeyGateway,
    vault: &'a dyn KeyVault,
    clock: &'a dyn Clock,
}

impl<'a> Keyring<'a> {
    pub fn new(
        gateway: &'a dyn KeyGateway,
        vault: &'a dyn KeyVault,
        clock: &'a dyn Clock,
    ) -> Keyring<'a> {
        Keyring {
            gateway,
            vault,
            clock,
        }
    }

    pub fn list(&self) -> Vec<Result<DeployKey, String>> {
        self.vault.list()
    }

    pub fn load(&self, at: &KeyRef) -> Result<DeployKey, KeyError> {
        self.vault.load(at)
    }

    pub fn private_key(&self, at: &KeyRef) -> Result<Secret, KeyError> {
        self.vault.private_key(at)
    }

    /// 새 배포 키. 로컬에 만들고 GitHub 에 등록한다.
    pub fn create(
        &self,
        account: &str,
        at: &KeyRef,
        write: bool,
        progress: &dyn ProgressSink,
    ) -> Result<DeployKey, KeyError> {
        if self.vault.exists(at) {
            return Err(KeyError::Taken(at.slug()));
        }

        let comment = format!("secrets/{}", at.slug());
        let material = self.vault.stage(at, &comment)?;
        // 등록이 실패해도 개인 키는 제자리에 남는다. 재시도할 수 있어야 하기 때문이다.
        self.vault.place(at)?;

        let key = DeployKey {
            purpose: at.purpose.clone(),
            repo: at.repo.slug(),
            account: account.to_string(),
            write,
            algorithm: material.algorithm,
            fingerprint: material.fingerprint,
            comment,
            created_at: self.clock.today(),
            state: KeyState::Local,
            remote_id: None,
            registered_at: None,
            retiring_remote_id: None,
        };
        self.vault.record(&key)?;

        self.register(key, account, at, &material.public_key, write, progress)
    }

    /// 등록만 다시 시도한다. 개인 키는 이미 제자리에 있다.
    pub fn retry(&self, at: &KeyRef, progress: &dyn ProgressSink) -> Result<DeployKey, KeyError> {
        let key = self.vault.load(at)?;
        if key.state != KeyState::Local {
            return Ok(key);
        }
        let public_key = self.vault.public_key(at)?;
        let account = key.account.clone();
        let write = key.write;
        self.register(key, &account, at, &public_key, write, progress)
    }

    fn register(
        &self,
        mut key: DeployKey,
        account: &str,
        at: &KeyRef,
        public_key: &str,
        write: bool,
        progress: &dyn ProgressSink,
    ) -> Result<DeployKey, KeyError> {
        let remote_id = self
            .gateway
            .register(account, at, public_key, write, progress)?;

        key.state = KeyState::Registered;
        key.remote_id = Some(remote_id);
        key.registered_at = Some(self.clock.now());
        self.vault.record(&key)?;
        Ok(key)
    }

    /// 재발급. 새 키를 먼저 등록하고 옛 키를 나중에 지운다.
    pub fn rotate(&self, at: &KeyRef, progress: &dyn ProgressSink) -> Result<DeployKey, KeyError> {
        let mut key = self.vault.load(at)?;
        if key.state == KeyState::Rotating {
            return self.finish_rotation(key, at, progress);
        }

        let material = self.vault.stage(at, &key.comment)?;
        let staged = self.vault.staged_public_key(at)?;

        // 새 키가 등록되기 전에는 아무것도 건드리지 않는다. 여기서 죽으면
        // 옛 키가 그대로 살아 있고 대기 자리만 남는다.
        let fresh = match self
            .gateway
            .register(&key.account, at, &staged, key.write, progress)
        {
            Ok(id) => id,
            Err(e) => {
                self.vault.discard_staged(at);
                return Err(e);
            }
        };

        self.vault.place(at)?;
        key.retiring_remote_id = key.remote_id.take();
        key.remote_id = Some(fresh);
        key.registered_at = Some(self.clock.now());
        key.fingerprint = material.fingerprint;
        key.algorithm = material.algorithm;
        key.state = KeyState::Rotating;
        self.vault.record(&key)?;

        self.finish_rotation(key, at, progress)
    }

    /// 옛 키를 GitHub 에서 지우고 재발급을 닫는다. 멈춘 자리에서 다시 부를 수 있다.
    fn finish_rotation(
        &self,
        mut key: DeployKey,
        at: &KeyRef,
        progress: &dyn ProgressSink,
    ) -> Result<DeployKey, KeyError> {
        if let Some(old) = key.retiring_remote_id.clone() {
            self.gateway
                .unregister(&key.account, &at.repo, &old, progress)?;
        }
        key.retiring_remote_id = None;
        key.state = KeyState::Registered;
        self.vault.record(&key)?;
        Ok(key)
    }

    /// 이 키가 무엇에 쓰이는지를 바꾼다.
    ///
    /// 로컬에서만 일어난다. GitHub 의 제목은 등록할 때 정해진 채로 둔다 — 고치려면
    /// 지웠다 다시 올려야 하는데, 그 사이 접속이 끊기고 얻는 것은 표시명뿐이다.
    /// 어느 등록이 이 키인지는 지문으로 맞출 수 있고, 지문은 양쪽 모두에 있다.
    pub fn set_purpose(&self, at: &KeyRef, to: &str) -> Result<DeployKey, KeyError> {
        let target = KeyRef::new(at.repo.clone(), to)
            .ok_or_else(|| KeyError::Storage(format!("용도로 쓸 수 없습니다: {to}")))?;
        if target == *at {
            return self.vault.load(at);
        }
        if self.vault.exists(&target) {
            return Err(KeyError::Taken(target.slug()));
        }

        let mut key = self.vault.load(at)?;
        self.vault.move_to(at, &target)?;
        key.purpose = target.purpose.clone();
        self.vault.record(&key)?;
        Ok(key)
    }

    /// 키를 걷어낸다. GitHub 에서 먼저 지우고 실물은 보관한다.
    ///
    /// 순서가 중요하다 — 실물을 먼저 치우면 `remote_id` 를 잃고 GitHub 에 남은 키를
    /// 지울 방법이 사라진다.
    pub fn remove(&self, at: &KeyRef, progress: &dyn ProgressSink) -> Result<(), KeyError> {
        let key = self.vault.load(at)?;

        for id in [key.remote_id.clone(), key.retiring_remote_id.clone()]
            .into_iter()
            .flatten()
        {
            self.gateway
                .unregister(&key.account, &at.repo, &id, progress)?;
        }
        self.vault.archive(at, "삭제")
    }

    /// GitHub 에는 있는데 이 금고에 개인 키가 없는 것.
    ///
    /// 계정에 붙은 키와, 우리가 이미 키를 둔 리포의 배포 키까지 본다. 알지 못하는
    /// 리포의 배포 키는 보이지 않는다 — 그건 여기서 찾을 수 있는 범위 밖이다.
    pub fn unowned(
        &self,
        account: &str,
        progress: &dyn ProgressSink,
    ) -> Result<Vec<crate::key::RemoteKey>, KeyError> {
        let mine: Vec<DeployKey> = self.vault.list().into_iter().flatten().collect();
        let known: Vec<&str> = mine.iter().map(|k| k.fingerprint.as_str()).collect();

        let mut found = self.gateway.account_keys(account, progress)?;

        let mut repos: Vec<RepoRef> = Vec::new();
        for key in mine.iter().filter(|k| k.account == account) {
            if let Some(repo) = RepoRef::parse(&key.repo)
                && !repos.contains(&repo)
            {
                repos.push(repo);
            }
        }
        for repo in &repos {
            found.extend(self.gateway.deploy_keys(account, repo, progress)?);
        }

        found.retain(|remote| !known.contains(&remote.fingerprint.as_str()));
        Ok(found)
    }
}
