//! 가짜 CLI 를 PATH 에 심어 실제 실행 경로를 검사하는 하네스.
//!
//! 이 도구의 계약은 대부분 "무엇을 어떤 환경으로 실행했고, 비밀값이 어디로 갔는가"다.
//! 파서만 검사하면 그 계약은 한 줄도 검사되지 않는다. 가짜 실행 파일을 PATH 앞에 두면
//! `find_in_path` 의 탐색과 환경변수 격리까지 실제 코드 경로 그대로 지나간다.
//!
//! `PATH` 와 `SECRETS_HOME` 은 프로세스 전역이므로 [`Sandbox`] 는 잠금으로 직렬화한다.

#![allow(dead_code)]

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// 가짜 실행 파일이 호출 기록을 남길 위치를 알려 주는 환경변수.
const TRACE_ENV: &str = "FAKE_CLI_TRACE_DIR";

/// 가짜 실행 파일이 호출될 때마다 남기는 기록.
pub struct Call {
    pub argv: Vec<String>,
    pub env: HashMap<String, String>,
    pub stdin: String,
}

impl Call {
    /// 인자 어디에도 이 값이 없는가. 비밀값이 `ps` 에 드러나지 않는지 보는 데 쓴다.
    pub fn argv_contains(&self, needle: &str) -> bool {
        self.argv.iter().any(|a| a.contains(needle))
    }
}

/// 격리된 `SECRETS_HOME` 과 가짜 PATH 를 가진 테스트 환경.
///
/// Drop 될 때 환경변수를 되돌리고 임시 트리를 지운다.
pub struct Sandbox {
    _guard: MutexGuard<'static, ()>,
    root: PathBuf,
    bin: PathBuf,
    trace: PathBuf,
    previous_path: Option<OsString>,
    previous_root: Option<OsString>,
}

fn lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Sandbox {
    pub fn new(label: &str) -> Sandbox {
        let guard = lock();

        let root = std::env::temp_dir().join(format!("secrets-contract-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        let bin = root.join("fake-bin");
        let trace = root.join("trace");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&trace).unwrap();

        let previous_path = std::env::var_os("PATH");
        let previous_root = std::env::var_os(secrets_local::vault::ROOT_ENV);

        let mut search = vec![bin.clone()];
        if let Some(existing) = &previous_path {
            search.extend(std::env::split_paths(existing));
        }
        let joined = std::env::join_paths(search).unwrap();

        // SAFETY: 잠금을 쥐고 있어 이 시점에 다른 테스트가 환경변수를 읽거나 쓰지 않는다.
        unsafe {
            std::env::set_var("PATH", &joined);
            std::env::set_var(secrets_local::vault::ROOT_ENV, &root);
            std::env::set_var(TRACE_ENV, &trace);
        }

        Sandbox {
            _guard: guard,
            root,
            bin,
            trace,
            previous_path,
            previous_root,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 가짜 실행 파일을 PATH 에 심는다.
    ///
    /// `behavior` 는 인자를 보고 출력과 종료 코드를 정하는 POSIX 셸 조각이다.
    /// 호출 기록(argv·env·stdin)은 하네스가 먼저 남기므로 여기서 신경 쓰지 않는다.
    pub fn install(&self, tool: &str, behavior: &str) {
        let script = format!("{PREAMBLE}\n{behavior}\n");
        let path = self.bin.join(tool);
        std::fs::write(&path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
    }

    /// 이 툴이 불린 횟수.
    pub fn call_count(&self, tool: &str) -> usize {
        std::fs::read_to_string(self.trace.join(tool).join("count"))
            .ok()
            .and_then(|n| n.trim().parse().ok())
            .unwrap_or(0)
    }

    /// `n` 번째(1부터) 호출 기록.
    pub fn call(&self, tool: &str, n: usize) -> Call {
        let dir = self.trace.join(tool).join(format!("call-{n}"));
        assert!(dir.is_dir(), "{tool} 의 {n} 번째 호출이 없다");

        let argv = std::fs::read_to_string(dir.join("argv"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect();

        let env = std::fs::read_to_string(dir.join("env"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        let stdin = std::fs::read_to_string(dir.join("stdin")).unwrap_or_default();

        Call { argv, env, stdin }
    }

    /// 마지막 호출 기록.
    pub fn last_call(&self, tool: &str) -> Call {
        let n = self.call_count(tool);
        assert!(n > 0, "{tool} 이 한 번도 불리지 않았다");
        self.call(tool, n)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // SAFETY: 잠금을 아직 쥐고 있다.
        unsafe {
            match &self.previous_path {
                Some(value) => std::env::set_var("PATH", value),
                None => std::env::remove_var("PATH"),
            }
            match &self.previous_root {
                Some(value) => std::env::set_var(secrets_local::vault::ROOT_ENV, value),
                None => std::env::remove_var(secrets_local::vault::ROOT_ENV),
            }
            std::env::remove_var(TRACE_ENV);
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// 호출 기록을 남기는 앞부분. 모든 가짜 실행 파일이 공유한다.
const PREAMBLE: &str = r#"#!/bin/sh
tool=$(basename "$0")
dir="$FAKE_CLI_TRACE_DIR/$tool"
mkdir -p "$dir"
n=$(cat "$dir/count" 2>/dev/null || echo 0)
n=$((n + 1))
printf '%s' "$n" > "$dir/count"
call="$dir/call-$n"
mkdir -p "$call"
: > "$call/argv"
for arg in "$@"; do printf '%s\n' "$arg" >> "$call/argv"; done
env > "$call/env"
cat > "$call/stdin"
"#;
