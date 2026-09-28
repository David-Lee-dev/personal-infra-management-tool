//! 서버에 들어가 스크립트를 돌린다.
//!
//! 셸을 거치지 않고 `ssh` 를 직접 실행한다. 스크립트는 **stdin 으로** 넘긴다 —
//! 명령행에 실으면 `ps` 로 읽히고 인용 문제가 생긴다.

use secrets_core::port::{Channel, ProgressSink};
use secrets_core::server::{Access, Install, Readiness, ServerError, ServerGateway};

use crate::cli::{exec, tools};
use crate::hosts::script;

pub struct SshHosts;

/// 처음 붙는 호스트의 키를 자동으로 받는다.
///
/// 물어보면 사람이 없는 자리에서 멈춘다. `accept-new` 는 처음 것만 받고 **바뀐
/// 키는 그대로 거절**하므로, 가로채기에는 열리지 않는다.
const COMMON: &[&str] = &[
    "-o",
    "StrictHostKeyChecking=accept-new",
    "-o",
    "BatchMode=yes",
    "-o",
    "ConnectTimeout=10",
];

/// ssh 에 넘길 인자 — 포트 · 키 · 대상. 키가 없으면 ssh 기본 키(ssh-agent · `~/.ssh/id_*`)에 맡긴다.
fn target_args(access: &Access) -> Vec<String> {
    let mut args = Vec::new();
    if access.port != 22 {
        args.extend(["-p".to_string(), access.port.to_string()]);
    }
    if let Some(key) = &access.key {
        args.extend([
            "-i".to_string(),
            key.clone(),
            "-o".to_string(),
            "IdentitiesOnly=yes".to_string(),
        ]);
    }
    args
}

/// 그 계정으로 들어가 스크립트를 stdin 으로 돌리고 stdout 을 돌려준다. 프로젝트의 서버 읽기 ·
/// 배포도 이 통로를 쓴다.
pub(crate) fn run(
    access: &Access,
    script: &str,
    progress: &dyn ProgressSink,
) -> Result<String, ServerError> {
    let program = tools::find_in_path("ssh")
        .ok_or_else(|| ServerError::Remote("ssh 를 찾을 수 없습니다".into()))?;

    let target = format!("{}@{}", access.login, access.address);
    let owned = target_args(access);
    let mut args: Vec<&str> = owned.iter().map(String::as_str).collect();
    args.extend_from_slice(COMMON);
    args.push("--");
    args.push(&target);
    args.push("bash -s");

    progress.line(Channel::Out, &format!("$ ssh {target} …"));

    let out = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = out.clone();
    let trouble = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let noted = trouble.clone();

    let outcome = exec::run_full(
        &program,
        &args,
        &[],
        Some(script.as_bytes()),
        move |stream, line| match stream {
            exec::Stream::Stdout => {
                let mut held = sink.lock().unwrap();
                held.push_str(&line);
                held.push('\n');
                progress.line(Channel::Out, &line);
            }
            exec::Stream::Stderr => {
                progress.line(Channel::Err, &line);
                *noted.lock().unwrap() = line;
            }
        },
    )
    .map_err(|e| ServerError::Remote(e.to_string()))?;

    let text = out.lock().unwrap().clone();
    if !outcome.ok() {
        let said = trouble.lock().unwrap().clone();
        let detail = if said.trim().is_empty() {
            format!("ssh 가 실패했습니다 ({})", outcome.code.unwrap_or(-1))
        } else {
            said.trim().to_string()
        };
        return Err(ServerError::Remote(detail));
    }
    Ok(text)
}

/// `has:…` · `packager:…` 줄을 읽는다.
fn readiness(text: &str) -> Readiness {
    let has = |what: &str| {
        text.lines()
            .any(|line| line.trim() == format!("has:{what}"))
    };
    Readiness {
        sudo: has("sudo"),
        acl: has("setfacl"),
        useradd: has("useradd"),
        visudo: has("visudo"),
        packager: text
            .lines()
            .find_map(|line| line.trim().strip_prefix("packager:").map(str::to_string)),
    }
}

impl ServerGateway for SshHosts {
    fn inspect(
        &self,
        admin: &Access,
        progress: &dyn ProgressSink,
    ) -> Result<Readiness, ServerError> {
        let text = run(admin, &script::inspect(), progress)?;
        Ok(readiness(&text))
    }

    fn prepare(
        &self,
        admin: &Access,
        progress: &dyn ProgressSink,
    ) -> Result<Readiness, ServerError> {
        run(admin, &script::prepare(), progress)?;
        self.inspect(admin, progress)
    }

    fn install(
        &self,
        admin: &Access,
        install: &Install,
        progress: &dyn ProgressSink,
    ) -> Result<bool, ServerError> {
        let text = run(
            admin,
            &script::install(
                &install.login,
                install.role.id(),
                &install.workspace,
                &install.group,
                &install.public_key,
                install.mode,
            ),
            progress,
        )
        .map_err(|e| match e {
            ServerError::Remote(said) if said == "account-taken" => ServerError::Invalid(format!(
                "서버에 {} 계정이 이미 있습니다. 다른 이름을 입력하세요.",
                install.login
            )),
            ServerError::Remote(said) if said.contains("group-grants-sudo") => ServerError::Remote(format!(
                "서버의 {0} 그룹에는 sudo 권한이 있습니다. user 역할의 계정에는 이 이름을 사용할 수 없습니다.",
                install.login
            )),
            other => other,
        })?;

        if !text.lines().any(|line| line.trim() == "ok") {
            return Err(ServerError::Remote(
                "스크립트가 완료되지 않았습니다.".into(),
            ));
        }
        // 있던 계정이면 지울 때 계정은 남긴다.
        Ok(text.lines().any(|line| line.trim() == "account-created"))
    }

    fn verify(
        &self,
        access: &Access,
        check_sudo: bool,
        progress: &dyn ProgressSink,
    ) -> Result<(), ServerError> {
        // 관리자면 sudo 까지 확인한다. 규칙을 넣고도 안 되는 경우가 있다.
        let check = if check_sudo {
            "sudo -n true && echo sudo-ok\necho ok\n"
        } else {
            "echo ok\n"
        };

        let text =
            run(access, check, progress).map_err(|e| ServerError::Unreachable(e.to_string()))?;

        if !text.lines().any(|line| line.trim() == "ok") {
            return Err(ServerError::Unreachable(
                "서버에 접속했지만 응답을 받지 못했습니다.".into(),
            ));
        }
        if check_sudo && !text.lines().any(|line| line.trim() == "sudo-ok") {
            return Err(ServerError::Unreachable(
                "sudo 명령을 실행할 수 없습니다.".into(),
            ));
        }
        Ok(())
    }

    fn remove(
        &self,
        admin: &Access,
        login: &str,
        group: &str,
        delete_account: bool,
        progress: &dyn ProgressSink,
    ) -> Result<(), ServerError> {
        let text = run(
            admin,
            &script::remove(login, group, delete_account),
            progress,
        )?;
        if !text.lines().any(|line| line.trim() == "ok") {
            return Err(ServerError::Remote(
                "스크립트가 끝까지 돌지 않았습니다".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access(key: Option<&str>, port: u16) -> Access {
        Access {
            key: key.map(str::to_string),
            login: "deploy".into(),
            address: "1.2.3.4".into(),
            port,
        }
    }

    #[test]
    fn a_key_is_the_only_one_offered_and_the_default_port_is_left_out() {
        assert_eq!(
            target_args(&access(Some("/k/key"), 22)),
            vec!["-i", "/k/key", "-o", "IdentitiesOnly=yes"]
        );
    }

    #[test]
    fn without_a_key_ssh_uses_its_own_defaults_and_another_port_is_passed() {
        assert_eq!(target_args(&access(None, 2222)), vec!["-p", "2222"]);
    }
}
