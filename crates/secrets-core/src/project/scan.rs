//! 디렉토리를 읽은 결과. 값은 없고 이름 · 개수 · 근거 파일만 있다.

use super::env_file::{self, EnvFileView};
use super::runtime::{RuntimeEvidence, RuntimeVerdict};

/// git 이 어디까지 붙었는가.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitState {
    /// 이 디렉토리가 저장소의 뿌리가 아니다.
    Absent,
    /// 저장소는 있지만 `origin` 이 없다.
    Local {
        /// 커밋이 없어도 브랜치 이름은 있다. 분리된 HEAD 면 없다.
        branch: Option<String>,
        commits: u32,
        changes: u32,
    },
    Remote {
        branch: Option<String>,
        commits: u32,
        changes: u32,
        /// `origin` 주소 그대로.
        origin: String,
    },
}

/// 뿌리에 있는 `.env*` 파일 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvFileFact {
    pub name: String,
    /// 변수 줄의 개수. 값은 세지 않는다.
    pub variables: usize,
    /// git 이 추적한다.
    pub tracked: bool,
    /// `.gitignore` 로 제외된다. git 저장소가 아니면 알 수 없다.
    pub ignored: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalScan {
    pub git: GitState,
    pub runtimes: Vec<RuntimeEvidence>,
    pub env_files: Vec<EnvFileFact>,
    /// 이 레포의 `core.sshCommand` 가 `-i` 로 가리키는 개인 키 파일.
    pub ssh_key: Option<String>,
}

impl LocalScan {
    pub fn runtime(&self) -> RuntimeVerdict {
        RuntimeVerdict::from_evidence(&self.runtimes)
    }

    /// 환경 변수 파일마다 무슨 역할인지와 노출 여부를 붙인다.
    pub fn env_view(&self) -> Vec<EnvFileView> {
        self.env_files.iter().map(env_file::view).collect()
    }
}
