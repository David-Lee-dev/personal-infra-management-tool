//! GitHub 에 레포를 만든다. 그 계정의 격리 홈으로 `gh` 를 돌린다.

use secrets_core::key::RepoRef;
use secrets_core::port::{Channel, ProgressSink};
use secrets_core::project::{ProjectError, RemoteRepos, Visibility, git_link::ssh_url};

use crate::cli::{exec, tools};
use crate::vault::paths;

pub struct GhRepos;

impl RemoteRepos for GhRepos {
    fn create(
        &self,
        account: &str,
        repo: &RepoRef,
        visibility: Visibility,
        progress: &dyn ProgressSink,
    ) -> Result<String, ProjectError> {
        let program = tools::find_in_path("gh")
            .ok_or_else(|| ProjectError::Storage("gh를 찾을 수 없습니다.".into()))?;
        let home = paths::cli_home_of(secrets_core::account::Provider::Github, account);
        let env = paths::env_for(secrets_core::account::Provider::Github, &home);

        let slug = repo.slug();
        let flag = match visibility {
            Visibility::Private => "--private",
            Visibility::Public => "--public",
        };
        let args = ["repo", "create", slug.as_str(), flag];
        progress.line(Channel::Out, &format!("$ {}", exec::display("gh", &args)));

        let outcome = exec::run_env(&program, &args, &env, |stream, line| {
            let channel = match stream {
                exec::Stream::Stdout => Channel::Out,
                exec::Stream::Stderr => Channel::Err,
            };
            progress.line(channel, &line);
        })
        .map_err(|e| ProjectError::Storage(e.to_string()))?;
        if !outcome.ok() {
            return Err(ProjectError::Storage(format!(
                "GitHub에 {slug}을(를) 만들지 못했습니다. 작업 로그에서 이유를 확인하세요."
            )));
        }
        Ok(ssh_url(repo))
    }
}
