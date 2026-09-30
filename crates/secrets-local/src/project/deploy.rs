//! 배포 스크립트 — `projects/<프로젝트>/deploy/<환경>/<스크립트>.sh` 보관과 서버에서의 실행.
//!
//! 고치거나 빼면 이전 스크립트를 `archive/projects/<프로젝트>/deploy/<환경>/<스크립트>-<시각>.sh` 로
//! 옮긴다. 사용자가 편집기로 직접 고치거나 `.sh` 파일을 더해도 된다. 도구는 이 파일들을 그대로 읽는다.
//!
//! 실행은 서버 계정으로 들어가 스크립트를 서버의 임시 파일에 다 쓴 뒤 `bash <파일> </dev/null` 로
//! 돌린다. stdin 으로 바로 흘리면 스크립트 안의 명령이 stdin 을 읽을 때 뒤따르는 줄을 삼킨다.

use std::path::{Path, PathBuf};

use secrets_core::port::ProgressSink;
use secrets_core::project::{
    DeployRunner, DeployScripts, ProjectError, ServerSeat, check_script_name,
};

use crate::hosts::{script, ssh};
use crate::{clock, vault};

pub struct FileDeployScripts;

fn storage(e: impl std::fmt::Display) -> ProjectError {
    ProjectError::Storage(e.to_string())
}

fn dir_of(project: &str, environment: &str) -> PathBuf {
    vault::root()
        .join("projects")
        .join(project)
        .join("deploy")
        .join(environment)
}

fn archive_of(project: &str, environment: &str) -> PathBuf {
    vault::root()
        .join("archive/projects")
        .join(project)
        .join("deploy")
        .join(environment)
}

/// 프로젝트 · 환경 이름은 기록에서 검사를 거친 것이지만, 경로로 쓰기 전에 한 번 더 막는다.
fn check(part: &str) -> Result<(), ProjectError> {
    if part.is_empty() || part.contains('/') || part == "." || part == ".." {
        return Err(ProjectError::Invalid(format!(
            "{part}은(는) 경로 이름으로 쓸 수 없습니다."
        )));
    }
    Ok(())
}

fn file_of(script: &str) -> String {
    format!("{script}.sh")
}

/// 이전 스크립트를 보관소로 옮긴다(복사 뒤 지우지는 않는다 — 부르는 쪽이 덮어쓰거나 지운다).
fn keep(
    project: &str,
    environment: &str,
    script: &str,
    path: &Path,
) -> Result<String, ProjectError> {
    let kept_dir = archive_of(project, environment);
    vault::create_private(&kept_dir).map_err(storage)?;
    let kept = kept_dir.join(format!("{script}-{}.sh", clock::stamp()));
    std::fs::copy(path, &kept).map_err(storage)?;
    vault::restrict(&kept).map_err(storage)?;
    Ok(kept.display().to_string())
}

impl DeployScripts for FileDeployScripts {
    fn names(&self, project: &str, environment: &str) -> Result<Vec<String>, ProjectError> {
        check(project)?;
        check(environment)?;
        let dir = dir_of(project, environment);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(storage(format!(
                    "{}을(를) 읽지 못했습니다: {e}",
                    dir.display()
                )));
            }
        };
        // 이름 규칙에 맞는 `<이름>.sh` 파일만 스크립트다. 쓰는 중인 임시 파일은 끝이 다르다.
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().is_file())
            .filter_map(|e| {
                let file = e.file_name().into_string().ok()?;
                let stem = file.strip_suffix(".sh")?;
                check_script_name(stem).ok().filter(|n| n == stem)
            })
            .collect();
        names.sort();
        Ok(names)
    }

    fn location(&self, project: &str, environment: &str, script: &str) -> String {
        dir_of(project, environment)
            .join(file_of(script))
            .display()
            .to_string()
    }

    fn load(
        &self,
        project: &str,
        environment: &str,
        script: &str,
    ) -> Result<Option<String>, ProjectError> {
        check(project)?;
        check(environment)?;
        check(script)?;
        let path = dir_of(project, environment).join(file_of(script));
        match std::fs::read_to_string(&path) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(storage(format!(
                "{}을(를) 읽지 못했습니다: {e}",
                path.display()
            ))),
        }
    }

    fn save(
        &self,
        project: &str,
        environment: &str,
        script: &str,
        text: &str,
    ) -> Result<Option<String>, ProjectError> {
        check(project)?;
        check(environment)?;
        check(script)?;
        let dir = dir_of(project, environment);
        let path = dir.join(file_of(script));
        vault::create_private(&dir).map_err(storage)?;

        let archived = if path.is_file() {
            Some(keep(project, environment, script, &path)?)
        } else {
            None
        };

        let staging = dir.join(format!("{}.writing", file_of(script)));
        vault::write_private(&staging, text.as_bytes()).map_err(storage)?;
        if let Err(e) = std::fs::rename(&staging, &path) {
            let _ = std::fs::remove_file(&staging);
            return Err(storage(e));
        }
        Ok(archived)
    }

    fn remove(
        &self,
        project: &str,
        environment: &str,
        script: &str,
    ) -> Result<Option<String>, ProjectError> {
        check(project)?;
        check(environment)?;
        check(script)?;
        let path = dir_of(project, environment).join(file_of(script));
        if !path.is_file() {
            return Ok(None);
        }
        let kept = keep(project, environment, script, &path)?;
        std::fs::remove_file(&path).map_err(storage)?;
        Ok(Some(kept))
    }
}

/// heredoc 끝 표시. 스크립트에 이 줄이 있으면 실행하지 않는다.
const SCRIPT_END: &str = "SECRETS_DEPLOY_SCRIPT_END";

/// 서버에서 돌릴 것 — 변수를 내보내고, 스크립트를 임시 파일에 쓴 뒤 stdin 없이 돌린다.
fn wrapper(variables: &[(&str, String)], body: &str) -> Result<String, ProjectError> {
    if body.lines().any(|l| l == SCRIPT_END) {
        return Err(ProjectError::Invalid(format!(
            "배포 스크립트에 {SCRIPT_END} 줄이 있어 실행할 수 없습니다."
        )));
    }
    let exports: String = variables
        .iter()
        .map(|(name, value)| format!("export {name}={}\n", script::quote(value)))
        .collect();
    Ok(format!(
        "set -u\n{exports}f=$(mktemp)\ntrap 'rm -f \"$f\"' EXIT\ncat > \"$f\" <<'{SCRIPT_END}'\n{body}\n{SCRIPT_END}\nbash \"$f\" </dev/null\n",
        body = body.trim_end_matches('\n'),
    ))
}

/// 환경의 서버 계정으로 스크립트를 돌린다. 출력은 작업 로그로 그대로 흐른다.
pub struct SshDeploy;

impl DeployRunner for SshDeploy {
    fn run(
        &self,
        seat: &ServerSeat,
        variables: &[(&str, String)],
        body: &str,
        progress: &dyn ProgressSink,
    ) -> Result<(), ProjectError> {
        let text = wrapper(variables, body)?;
        ssh::run(&super::server::access_of(seat)?, &text, progress)
        .map(|_| ())
        .map_err(|e| ProjectError::Storage(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::tests_support::with_temp_root;

    #[test]
    fn writes_privately_and_keeps_the_previous_script() {
        with_temp_root(|root| {
            let scripts = FileDeployScripts;
            assert_eq!(scripts.load("api", "prod", "script").unwrap(), None);

            assert_eq!(
                scripts.save("api", "prod", "script", "echo 1\n").unwrap(),
                None
            );
            let archived = scripts
                .save("api", "prod", "script", "echo 2\n")
                .unwrap()
                .unwrap();

            assert_eq!(
                scripts.load("api", "prod", "script").unwrap().as_deref(),
                Some("echo 2\n")
            );
            assert_eq!(std::fs::read_to_string(&archived).unwrap(), "echo 1\n");
            assert!(
                archived.starts_with(
                    &root
                        .join("archive/projects/api/deploy/prod")
                        .display()
                        .to_string()
                )
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = |p: PathBuf| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
                assert_eq!(mode(root.join("projects/api/deploy/prod/script.sh")), 0o600);
                assert_eq!(mode(root.join("projects/api/deploy/prod")), 0o700);
                assert_eq!(mode(PathBuf::from(&archived)), 0o600);
            }
            let left: Vec<_> = std::fs::read_dir(root.join("projects/api/deploy/prod"))
                .unwrap()
                .collect();
            assert_eq!(left.len(), 1, "임시 파일이 남으면 안 된다");
        });
    }

    mod wrapper {
        use super::*;

        fn run_here(text: &str) -> std::process::Output {
            std::process::Command::new("bash")
                .arg("-s")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .and_then(|mut child| {
                    use std::io::Write;
                    child.stdin.take().unwrap().write_all(text.as_bytes())?;
                    child.wait_with_output()
                })
                .unwrap()
        }

        #[test]
        fn exports_variables_and_keeps_the_scripts_exit_code() {
            let vars = [
                ("DEPLOY_PATH", "/srv/it's here".to_string()),
                ("DEPLOY_ENV", "prod".to_string()),
            ];
            let out =
                run_here(&wrapper(&vars, "echo \"$DEPLOY_ENV:$DEPLOY_PATH\"\nexit 3\n").unwrap());
            assert_eq!(
                String::from_utf8_lossy(&out.stdout),
                "prod:/srv/it's here\n"
            );
            assert_eq!(out.status.code(), Some(3));
        }

        #[test]
        fn a_command_reading_stdin_does_not_swallow_the_rest() {
            let out = run_here(&wrapper(&[], "cat\necho after\n").unwrap());
            assert_eq!(String::from_utf8_lossy(&out.stdout), "after\n");
            assert!(out.status.success());
        }

        #[test]
        fn refuses_a_script_holding_the_end_marker() {
            assert!(wrapper(&[], "echo\nSECRETS_DEPLOY_SCRIPT_END\n").is_err());
        }
    }

    #[test]
    fn lists_named_scripts_and_removes_one_to_the_archive() {
        with_temp_root(|root| {
            let scripts = FileDeployScripts;
            assert!(scripts.names("api", "prod").unwrap().is_empty());
            scripts.save("api", "prod", "migrate", "echo m\n").unwrap();
            scripts.save("api", "prod", "deploy", "echo d\n").unwrap();
            let dir = root.join("projects/api/deploy/prod");
            std::fs::write(dir.join("notes.txt"), "x").unwrap();
            std::fs::write(dir.join("bad name.sh"), "x").unwrap();

            assert_eq!(scripts.names("api", "prod").unwrap(), ["deploy", "migrate"]);

            let kept = scripts.remove("api", "prod", "migrate").unwrap().unwrap();
            assert_eq!(std::fs::read_to_string(&kept).unwrap(), "echo m\n");
            assert!(kept.contains("archive/projects/api/deploy/prod/migrate-"));
            assert_eq!(scripts.names("api", "prod").unwrap(), ["deploy"]);
            assert_eq!(scripts.remove("api", "prod", "migrate").unwrap(), None);
        });
    }

    #[test]
    fn refuses_path_like_names() {
        with_temp_root(|_| {
            assert!(
                FileDeployScripts
                    .save("..", "prod", "script", "x\n")
                    .is_err()
            );
            assert!(FileDeployScripts.load("api", "a/b", "script").is_err());
            assert!(FileDeployScripts.load("api", "prod", "..").is_err());
        });
    }
}
