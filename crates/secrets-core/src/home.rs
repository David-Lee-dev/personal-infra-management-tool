//! 이 도구가 만지는 모든 파일의 뿌리.
//!
//! 자격 증명 관리 도구가 파일을 여기저기 흩뿌리면 그 자체가 원래 해결하려던 문제가
//! 된다. 캐시·임시 파일·계정 설정·CLI 격리 홈까지 전부 이 아래에만 만든다.
//! `/tmp` 나 `~/Library` 같은 곳은 쓰지 않는다.
//!
//! ```text
//! ~/.secrets/
//!   accounts/    계정 레지스트리
//!   contexts/    컨텍스트와 CLI 격리 홈
//!   cache/       재생성 가능한 것만. 지워도 안전하다
//!   tmp/         프로브용 임시 디렉토리. 사용 후 즉시 지운다
//! ```

use std::io;
use std::path::{Path, PathBuf};

/// 뿌리 위치를 바꾸는 환경변수. 테스트와 다중 프로필에 쓴다.
pub const ROOT_ENV: &str = "SECRETS_HOME";

pub const ACCOUNTS: &str = "accounts";
pub const CONTEXTS: &str = "contexts";
pub const CACHE: &str = "cache";
pub const TMP: &str = "tmp";

/// 뿌리 경로. 디렉토리를 만들지는 않는다.
pub fn root() -> PathBuf {
    if let Some(custom) = std::env::var_os(ROOT_ENV) {
        return PathBuf::from(custom);
    }
    home_dir().join(".secrets")
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        // HOME 이 없는 환경은 사실상 없지만, 있더라도 현재 디렉토리를 오염시키지 않는다.
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// 뿌리와 하위 디렉토리를 만들고 권한을 조인다.
pub fn ensure() -> io::Result<PathBuf> {
    let root = root();
    create_private(&root)?;
    for sub in [ACCOUNTS, CONTEXTS, CACHE, TMP] {
        create_private(&root.join(sub))?;
    }
    Ok(root)
}

/// 소유자만 접근 가능한 디렉토리를 만든다. 이미 있으면 권한만 다시 조인다.
pub fn create_private(path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)?;
    restrict(path)
}

#[cfg(unix)]
pub fn restrict(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path)?;
    let mode = if meta.is_dir() { 0o700 } else { 0o600 };
    // 이미 맞으면 건드리지 않는다. mtime 을 괜히 흔들지 않기 위해서다.
    if meta.permissions().mode() & 0o777 != mode {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn restrict(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// `tmp/` 아래의 임시 디렉토리. Drop 될 때 지워진다.
///
/// 프로브는 CLI 가 설정 파일을 어디에 쓰는지 보려고 실제 디렉토리를 필요로 한다.
/// 그 쓰레기가 남지 않도록 수명을 타입으로 묶는다.
pub struct Scratch {
    path: PathBuf,
}

impl Scratch {
    pub fn new(label: &str) -> io::Result<Scratch> {
        let root = ensure()?.join(TMP);
        // 같은 라벨로 동시에 두 번 만들 일은 없지만, 남아 있던 잔재는 치우고 시작한다.
        let path = root.join(label);
        if path.exists() {
            std::fs::remove_dir_all(&path)?;
        }
        create_private(&path)?;
        Ok(Scratch { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 디렉토리 안에 무엇이든 생겼는가. 프로브의 판정 근거가 된다.
    pub fn is_empty(&self) -> bool {
        match std::fs::read_dir(&self.path) {
            Ok(mut entries) => entries.next().is_none(),
            Err(_) => true,
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // 지우기 실패는 치명적이지 않다. 다음 실행에서 같은 라벨로 다시 치운다.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 테스트가 실제 `~/.secrets` 를 건드리지 않게 하는 헬퍼.
///
/// `SECRETS_HOME` 은 프로세스 전역이라 테스트가 병렬로 돌면 서로를 덮어쓴다.
/// 잠금으로 직렬화한다.
#[cfg(test)]
pub mod tests_support {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn with_temp_root<T>(body: impl FnOnce(&Path) -> T) -> T {
        let _guard = lock();

        let dir = std::env::temp_dir().join(format!(
            "secrets-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);

        // SAFETY: 잠금이 있어 이 시점에 다른 테스트가 환경변수를 읽거나 쓰지 않는다.
        unsafe { std::env::set_var(ROOT_ENV, &dir) };
        let result = body(&dir);
        unsafe { std::env::remove_var(ROOT_ENV) };

        let _ = std::fs::remove_dir_all(&dir);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tests_support::with_temp_root;

    #[test]
    fn ensure_creates_private_tree() {
        with_temp_root(|dir| {
            let root = ensure().unwrap();
            assert_eq!(root, dir);
            for sub in [ACCOUNTS, CONTEXTS, CACHE, TMP] {
                assert!(dir.join(sub).is_dir(), "{sub} 이 없다");
            }

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(dir).unwrap().permissions().mode();
                assert_eq!(mode & 0o777, 0o700);
            }
        });
    }

    #[test]
    fn scratch_is_removed_on_drop() {
        with_temp_root(|dir| {
            let path = {
                let scratch = Scratch::new("probe").unwrap();
                assert!(scratch.is_empty());
                std::fs::write(scratch.path().join("marker"), b"x").unwrap();
                assert!(!scratch.is_empty());
                scratch.path().to_path_buf()
            };
            assert!(!path.exists(), "Scratch 가 Drop 에서 지워지지 않았다");
            assert!(dir.join(TMP).is_dir());
        });
    }
}
