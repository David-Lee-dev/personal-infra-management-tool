//! 배포 키가 이 머신에서 사는 방식.
//!
//! ```text
//! paths/   ~/.secrets/keys 안의 자리
//! hosts/   이 머신이 아는 SSH 호스트 — 키를 어디로 보낼지 고르는 데 쓴다
//! vault/   개인 키 파일과 기록 — ssh-keygen 이 만든다
//! github/  GitHub 등록과 삭제 — gh 가 말한다
//! ```

pub mod github;
pub mod hosts;
pub mod paths;
pub mod vault;

pub use github::GhKeys;
pub use vault::FileKeys;

use std::io;
use std::path::Path;

/// 로컬 리포 디렉토리의 origin 주소.
///
/// 소유자와 리포 이름을 손으로 옮겨 적게 하면 그때마다 틀릴 수 있다.
/// 이미 그 디렉토리가 답을 갖고 있으므로 git 에게 묻는다.
pub fn origin_of(dir: &Path) -> io::Result<String> {
    let program = crate::cli::tools::find_in_path("git")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "git 을 찾을 수 없습니다"))?;

    let buffer = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = buffer.clone();

    let outcome = crate::cli::exec::run(
        &program,
        &["-C", &dir.display().to_string(), "remote", "get-url", "origin"],
        move |stream, line| {
            if stream == crate::cli::exec::Stream::Stdout {
                sink.lock().unwrap().push_str(line.trim());
            }
        },
    )?;

    let url = buffer.lock().unwrap().clone();
    if !outcome.ok() || url.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "이 디렉토리에 origin 이 없습니다",
        ));
    }
    Ok(url)
}
