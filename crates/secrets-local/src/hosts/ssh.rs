//! pem 으로 서버에 들어가 스크립트를 돌린다.
//!
//! 셸을 거치지 않고 `ssh` 를 직접 실행한다. 스크립트는 **stdin 으로** 넘긴다 —
//! 명령행에 실으면 `ps` 로 읽히고 인용 문제가 생긴다.

use secrets_core::aws::instance::{HostError, InstanceGateway, Plan, Readiness, Seat};
use secrets_core::port::{Channel, ProgressSink};

use crate::cli::{exec, tools};
use crate::hosts::script;

pub struct SshHosts;

/// 처음 붙는 호스트의 키를 자동으로 받는다.
///
/// 물어보면 사람이 없는 자리에서 멈춘다. `accept-new` 는 처음 것만 받고 **바뀐
/// 키는 그대로 거절**하므로, 가로채기에는 열리지 않는다.
const COMMON: &[&str] = &[
    "-o",
    "IdentitiesOnly=yes",
    "-o",
    "StrictHostKeyChecking=accept-new",
    "-o",
    "BatchMode=yes",
    "-o",
    "ConnectTimeout=10",
];

fn run(
    key: &str,
    login: &str,
    address: &str,
    script: &str,
    progress: &dyn ProgressSink,
) -> Result<String, HostError> {
    let program = tools::find_in_path("ssh")
        .ok_or_else(|| HostError::Remote("ssh 를 찾을 수 없습니다".into()))?;

    let target = format!("{login}@{address}");
    let mut args: Vec<&str> = vec!["-i", key];
    args.extend_from_slice(COMMON);
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
    .map_err(|e| HostError::Remote(e.to_string()))?;

    let text = out.lock().unwrap().clone();
    if !outcome.ok() {
        let said = trouble.lock().unwrap().clone();
        let detail = if said.trim().is_empty() {
            format!("ssh 가 실패했습니다 ({})", outcome.code.unwrap_or(-1))
        } else {
            said.trim().to_string()
        };
        return Err(HostError::Remote(detail));
    }
    Ok(text)
}

/// `has:…` · `packager:…` 줄을 읽는다.
fn readiness(text: &str) -> Readiness {
    let has = |what: &str| text.lines().any(|line| line.trim() == format!("has:{what}"));
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

impl InstanceGateway for SshHosts {
    fn inspect(
        &self,
        pem: &str,
        plan: &Plan,
        progress: &dyn ProgressSink,
    ) -> Result<Readiness, HostError> {
        let text = run(pem, &plan.via, &plan.address, &script::inspect(), progress)?;
        Ok(readiness(&text))
    }

    fn prepare(
        &self,
        pem: &str,
        plan: &Plan,
        progress: &dyn ProgressSink,
    ) -> Result<Readiness, HostError> {
        run(pem, &plan.via, &plan.address, &script::prepare(), progress)?;
        self.inspect(pem, plan, progress)
    }

    fn install(
        &self,
        pem: &str,
        seat: &Seat,
        plan: &Plan,
        public_key: &str,
        progress: &dyn ProgressSink,
    ) -> Result<bool, HostError> {
        let text = run(
            pem,
            &plan.via,
            &plan.address,
            &script::install(
                &seat.account,
                plan.role.id(),
                &plan.workspace,
                &plan.group,
                public_key,
            ),
            progress,
        )
        .map_err(|e| match e {
            HostError::Remote(said) if said.contains("group-grants-sudo") => HostError::Remote(format!(
                "서버의 {0} 그룹은 sudo 를 받습니다. user 역할 계정은 이 이름을 쓸 수 없습니다",
                seat.account
            )),
            other => other,
        })?;

        if !text.lines().any(|line| line.trim() == "ok") {
            return Err(HostError::Remote("스크립트가 끝까지 돌지 않았습니다".into()));
        }
        // 있던 계정이면 지울 때 계정은 남긴다.
        Ok(text.lines().any(|line| line.trim() == "account-created"))
    }

    fn verify(
        &self,
        private_key: &str,
        seat: &Seat,
        plan: &Plan,
        progress: &dyn ProgressSink,
    ) -> Result<(), HostError> {
        // 관리자면 sudo 까지 확인한다. 규칙을 넣고도 안 되는 경우가 있다.
        let check = if plan.role == secrets_core::aws::instance::Role::Admin {
            "sudo -n true && echo sudo-ok\necho ok\n"
        } else {
            "echo ok\n"
        };

        let text = run(private_key, &seat.account, &plan.address, check, progress)
            .map_err(|e| HostError::Unreachable(e.to_string()))?;

        if !text.lines().any(|line| line.trim() == "ok") {
            return Err(HostError::Unreachable("들어갔지만 응답이 없습니다".into()));
        }
        if plan.role == secrets_core::aws::instance::Role::Admin
            && !text.lines().any(|line| line.trim() == "sudo-ok")
        {
            return Err(HostError::Unreachable("sudo 가 통하지 않습니다".into()));
        }
        Ok(())
    }

    fn remove(
        &self,
        pem: &str,
        seat: &Seat,
        plan: &Plan,
        ours: bool,
        progress: &dyn ProgressSink,
    ) -> Result<(), HostError> {
        let text = run(
            pem,
            &plan.via,
            &plan.address,
            &script::remove(&seat.account, &plan.group, ours),
            progress,
        )?;

        if !text.lines().any(|line| line.trim() == "ok") {
            return Err(HostError::Remote("스크립트가 끝까지 돌지 않았습니다".into()));
        }
        Ok(())
    }
}
