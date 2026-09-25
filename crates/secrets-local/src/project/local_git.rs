//! 로컬 레포의 git 설정 — `origin` 과 이 레포에서만 쓰는 SSH 키.
//!
//! 전역 설정과 `~/.ssh/config` 는 건드리지 않는다. 전부 그 레포의 `.git/config` 에만 쓴다.

use std::path::Path;

use secrets_core::port::{Channel, ProgressSink};
use secrets_core::project::{LocalRepository, LocalRevisions, ProjectError, Revision};

use super::git::{Git, ssh_command};

pub struct LocalGit;

fn git() -> Result<Git, ProjectError> {
    Git::find().ok_or_else(|| ProjectError::Storage("git을 찾을 수 없습니다.".into()))
}

impl LocalRepository for LocalGit {
    fn set_origin(&self, path: &str, url: &str) -> Result<(), ProjectError> {
        git()?
            .write(Path::new(path), &["remote", "add", "origin", url])
            .map_err(|e| ProjectError::Storage(format!("origin을 설정하지 못했습니다: {e}")))
    }

    fn use_key(&self, path: &str, private_key: &str) -> Result<(), ProjectError> {
        let command = ssh_command(private_key).ok_or_else(|| {
            ProjectError::Invalid(format!(
                "{private_key}에 작은따옴표가 있어 설정할 수 없습니다."
            ))
        })?;
        git()?
            .write(
                Path::new(path),
                &["config", "--local", "core.sshCommand", &command],
            )
            .map_err(|e| ProjectError::Storage(format!("SSH 키를 지정하지 못했습니다: {e}")))
    }

    fn reach(&self, path: &str, progress: &dyn ProgressSink) -> Result<(), ProjectError> {
        progress.line(Channel::Out, "$ git ls-remote --heads origin");
        git()?
            .reach(Path::new(path), |stream, line| {
                let channel = match stream {
                    crate::cli::exec::Stream::Stdout => Channel::Out,
                    crate::cli::exec::Stream::Stderr => Channel::Err,
                };
                progress.line(channel, &line);
            })
            .map_err(ProjectError::Storage)
    }
}

/// 배포 전에 로컬 · 원격의 커밋을 읽는다. `fetch` 는 원격 추적 브랜치만 바꾼다.
impl LocalRevisions for LocalGit {
    fn fetch(
        &self,
        path: &str,
        branch: &str,
        progress: &dyn ProgressSink,
    ) -> Result<(), ProjectError> {
        progress.line(Channel::Out, &format!("$ git fetch origin {branch}"));
        git()?
            .write(Path::new(path), &["fetch", "--quiet", "origin", branch])
            .map_err(|e| {
                ProjectError::Storage(format!(
                    "origin에서 {branch}을(를) 가져오지 못했습니다: {e}"
                ))
            })
    }

    fn resolve(&self, path: &str, reference: &str) -> Option<Revision> {
        let (sha, subject) = Git::find()?.revision(Path::new(path), reference)?;
        Some(Revision { sha, subject })
    }

    fn count(&self, path: &str, from: &str, to: &str) -> Option<u32> {
        Git::find()?.count(Path::new(path), from, to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::LocalWorkspace;
    use crate::project::workspace::tests_support::TempDir;
    use secrets_core::port::Silent;
    use secrets_core::project::{GitState, Workspace};

    fn repo() -> Option<TempDir> {
        Git::find()?;
        let dir = TempDir::new("local-git");
        LocalWorkspace.init_git(&dir.text()).unwrap();
        Some(dir)
    }

    #[test]
    fn origin_and_key_written_here_are_what_the_scan_reads_back() {
        let Some(dir) = repo() else { return };

        LocalGit
            .set_origin(&dir.text(), "git@github.com:Org/api.git")
            .unwrap();
        LocalGit
            .use_key(&dir.text(), "/vault/Org/api/develop/key")
            .unwrap();

        let scan = LocalWorkspace.scan(&dir.text()).unwrap();
        assert!(
            matches!(scan.git, GitState::Remote { ref origin, .. } if origin == "git@github.com:Org/api.git")
        );
        assert_eq!(scan.ssh_key.as_deref(), Some("/vault/Org/api/develop/key"));
    }

    #[test]
    fn setting_origin_twice_is_refused_by_git() {
        let Some(dir) = repo() else { return };
        LocalGit
            .set_origin(&dir.text(), "git@github.com:Org/api.git")
            .unwrap();
        assert!(
            LocalGit
                .set_origin(&dir.text(), "git@github.com:Org/other.git")
                .is_err()
        );
    }

    #[test]
    fn reach_succeeds_against_a_reachable_origin_and_fails_otherwise() {
        let Some(dir) = repo() else { return };
        let bare = TempDir::new("bare");
        Git::find()
            .unwrap()
            .write(bare.path(), &["init", "--bare", "--quiet"])
            .unwrap();

        LocalGit.set_origin(&dir.text(), &bare.text()).unwrap();
        assert!(LocalGit.reach(&dir.text(), &Silent).is_ok());

        let lonely = repo().unwrap();
        LocalGit
            .set_origin(
                &lonely.text(),
                &bare.path().join("gone").display().to_string(),
            )
            .unwrap();
        assert!(LocalGit.reach(&lonely.text(), &Silent).is_err());
    }

    #[test]
    fn a_key_path_with_a_quote_is_refused() {
        let Some(dir) = repo() else { return };
        assert!(matches!(
            LocalGit.use_key(&dir.text(), "/vault/it's/key"),
            Err(ProjectError::Invalid(_))
        ));
    }
}
