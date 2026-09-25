//! 프로젝트 — 기록은 `~/.secrets/projects/<이름>/project.toml`, 디렉토리는 사용자의 작업 공간.
//!
//! ```text
//! store.rs      기록 읽기 · 새 기록 쓰기
//! workspace.rs  디렉토리 상태 · 만들기 · git init · 스캔
//! detect.rs     근거 파일에서 런타임 찾기
//! git.rs        git 질의와 로컬 설정 쓰기
//! local_git.rs  origin · 레포 전용 SSH 키 · 접속 확인
//! keys.rs       시크릿 저장소의 배포 키를 프로젝트에 넘긴다
//! github.rs     GitHub 레포 만들기
//! server.rs     서버 계정 목록 · 배포 경로 읽기
//! code.rs       서버에 레포 키를 두고 clone
//! env.rs        환경 변수 파일 해시 비교 · 서버 .env 쓰기
//! deploy.rs     배포 스크립트 보관 · 이전 스크립트 보관소로
//! ```

mod code;
mod deploy;
mod detect;
mod env;
mod git;
mod github;
mod keys;
mod local_git;
mod server;
mod store;
mod workspace;

pub use code::SshCode;
pub use deploy::{FileDeployScripts, SshDeploy};
pub use env::{LocalEnv, SshEnv};
pub use github::GhRepos;
pub use keys::VaultRepoKeys;
pub use local_git::LocalGit;
pub use server::{SshProbe, VaultSeats};
pub use store::FileProjects;
pub use workspace::{LocalWorkspace, absolute, inside, workspace_root};

#[cfg(test)]
pub(crate) use workspace::tests_support;
