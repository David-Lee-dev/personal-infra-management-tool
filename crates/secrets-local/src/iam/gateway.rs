//! 마스터 계정으로 AWS IAM 에 하는 일.
//!
//! 응답은 `--query` 와 `--output text` 로 받는다. 무엇을 꺼내 오는지가 명령에 적혀
//! 사람이 읽을 수 있다. 시크릿이 돌아오는 명령이 있으므로 stdout 은 진행 창에
//! 흘리지 않는다.

use secrets_core::aws::AwsError;
use secrets_core::aws::iam::{ExistingKey, ExistingUser, IamError, IamGateway, IamRef, LastUse, Probe};
use secrets_core::credential::secret::Secret;
use secrets_core::port::{Channel, ProgressSink};

use crate::aws::{aws, aws_with, value};
use crate::vault::Scratch;

pub struct CliIam;

/// 새 키가 AWS 에 퍼지기를 기다리는 횟수와 간격. 보통 몇 초면 된다.
const PROPAGATION_TRIES: u32 = 10;
const PROPAGATION_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

fn remote(e: AwsError) -> IamError {
    match e {
        AwsError::Remote(detail) => IamError::Remote(detail),
        other => IamError::Remote(other.to_string()),
    }
}

/// 이미 없는 것을 지우라고 했을 때. 지우는 일은 다시 불러도 같은 결과여야 한다.
fn already_gone(e: &IamError) -> bool {
    matches!(e, IamError::Remote(detail) if detail.contains("NoSuchEntity"))
}

/// `arn:aws:iam::123:user/path/name` → `name`. 사용자가 아니면 없다.
fn user_of_arn(arn: &str) -> Option<String> {
    let (_, rest) = arn.trim().split_once(":user/")?;
    rest.rsplit('/').next().filter(|name| !name.is_empty()).map(str::to_string)
}

/// `list-access-keys` 의 `[AccessKeyId,CreateDate]` 텍스트 줄들.
fn key_rows(text: &str) -> Vec<ExistingKey> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            match (parts.next(), parts.next()) {
                (Some(id), Some(at)) => Some(ExistingKey {
                    id: id.to_string(),
                    created_at: at.to_string(),
                }),
                _ => None,
            }
        })
        .collect()
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter(|w| *w != "None")
        .map(str::to_string)
        .collect()
}

impl CliIam {
    /// 키 하나를 지운다. 이미 없으면 지운 것으로 친다.
    fn delete_key(&self, master: &str, name: &str, key_id: &str, progress: &dyn ProgressSink) -> Result<(), IamError> {
        let done = self.run(
            master,
            &["iam", "delete-access-key", "--user-name", name, "--access-key-id", key_id],
            progress,
        );
        match done {
            Err(e) if already_gone(&e) => Ok(()),
            other => other.map(|_| ()),
        }
    }

    fn run(&self, master: &str, args: &[&str], progress: &dyn ProgressSink) -> Result<String, IamError> {
        aws(master, args, progress).map_err(remote)
    }

    fn keys_of(&self, master: &str, name: &str, progress: &dyn ProgressSink) -> Result<Vec<String>, IamError> {
        let text = self.run(
            master,
            &[
                "iam", "list-access-keys", "--user-name", name,
                "--query", "AccessKeyMetadata[].AccessKeyId", "--output", "text",
            ],
            progress,
        )?;
        Ok(words(&text))
    }

    fn inline_policies(&self, master: &str, name: &str, progress: &dyn ProgressSink) -> Result<Vec<String>, IamError> {
        let text = self.run(
            master,
            &["iam", "list-user-policies", "--user-name", name, "--query", "PolicyNames", "--output", "text"],
            progress,
        )?;
        Ok(words(&text))
    }

    fn attached_policies(&self, master: &str, name: &str, progress: &dyn ProgressSink) -> Result<Vec<String>, IamError> {
        let text = self.run(
            master,
            &[
                "iam", "list-attached-user-policies", "--user-name", name,
                "--query", "AttachedPolicies[].PolicyArn", "--output", "text",
            ],
            progress,
        )?;
        Ok(words(&text))
    }

    /// 그 키로 한 번 들어가 본다. 자격은 임시 파일로 넘긴다 — 명령행과 환경변수에 싣지 않는다.
    fn identify_once(&self, key_id: &str, secret: &Secret, progress: &dyn ProgressSink) -> Result<String, IamError> {
        let scratch = Scratch::new("iam-identify").map_err(|e| IamError::Storage(e.to_string()))?;
        let credentials = scratch.path().join("credentials");
        let config = scratch.path().join("config");
        let body = format!(
            "[default]\naws_access_key_id = {key_id}\naws_secret_access_key = {}\n",
            secret.expose()
        );
        write_private(&credentials, body.as_bytes())?;
        write_private(&config, b"[default]\nregion = us-east-1\n")?;

        let env = [
            ("AWS_SHARED_CREDENTIALS_FILE", credentials.display().to_string()),
            ("AWS_CONFIG_FILE", config.display().to_string()),
        ];
        let text = aws_with(
            &env,
            &["sts", "get-caller-identity", "--query", "Arn", "--output", "text"],
            progress,
        )
        .map_err(remote)?;
        value(&text).ok_or_else(|| IamError::Remote("ARN 을 읽지 못했습니다".into()))
    }
}

/// 표준 오류를 흘리지 않는 진행 창.
///
/// 새 키가 퍼지는 동안의 거절은 오류가 아니라 대기다. 그걸 빨간 줄로 찍으면 성공한
/// 일이 실패처럼 보인다. 끝내 실패하면 마지막 거절 문구가 오류 값으로 올라간다.
struct Waiting<'a>(&'a dyn ProgressSink);

impl ProgressSink for Waiting<'_> {
    fn line(&self, channel: Channel, text: &str) {
        if channel == Channel::Out {
            self.0.line(channel, text);
        }
    }
}

fn write_private(path: &std::path::Path, bytes: &[u8]) -> Result<(), IamError> {
    std::fs::write(path, bytes).map_err(|e| IamError::Storage(e.to_string()))?;
    crate::vault::restrict(path).map_err(|e| IamError::Storage(e.to_string()))
}

impl IamGateway for CliIam {
    fn account_id(&self, master: &str, progress: &dyn ProgressSink) -> Result<String, IamError> {
        let text = self.run(
            master,
            &["sts", "get-caller-identity", "--query", "Account", "--output", "text"],
            progress,
        )?;
        value(&text).ok_or_else(|| IamError::Remote("계정 ID 를 읽지 못했습니다".into()))
    }

    fn create_user(&self, master: &str, name: &str, progress: &dyn ProgressSink) -> Result<(), IamError> {
        self.run(
            master,
            &[
                "iam", "create-user", "--user-name", name,
                "--tags", "Key=managed-by,Value=secret-manager",
                "--query", "User.Arn", "--output", "text",
            ],
            progress,
        )
        .map(|_| ())
    }

    /// 정책 원문은 임시 파일로 넘긴다. 명령행에 실으면 진행 창이 JSON 으로 뒤덮인다.
    fn put_policy(&self, master: &str, name: &str, policy: &str, progress: &dyn ProgressSink) -> Result<(), IamError> {
        let scratch = Scratch::new("iam-policy").map_err(|e| IamError::Storage(e.to_string()))?;
        let file = scratch.path().join("policy.json");
        write_private(&file, policy.as_bytes())?;
        let document = format!("file://{}", file.display());
        self.run(
            master,
            &[
                "iam", "put-user-policy", "--user-name", name,
                "--policy-name", name, "--policy-document", &document,
            ],
            progress,
        )
        .map(|_| ())
    }

    fn allows(&self, master: &str, at: &IamRef, probe: &Probe, progress: &dyn ProgressSink) -> Result<bool, IamError> {
        let source = format!("arn:aws:iam::{}:user/{}", at.account, at.name);
        let text = self.run(
            master,
            &[
                "iam", "simulate-principal-policy",
                "--policy-source-arn", &source,
                "--action-names", &probe.action,
                "--resource-arns", &probe.resource,
                "--query", "EvaluationResults[0].EvalDecision", "--output", "text",
            ],
            progress,
        )?;
        Ok(text.trim() == "allowed")
    }

    fn issue_key(&self, master: &str, name: &str, progress: &dyn ProgressSink) -> Result<(String, Secret), IamError> {
        let text = self.run(
            master,
            &[
                "iam", "create-access-key", "--user-name", name,
                "--query", "AccessKey.[AccessKeyId,SecretAccessKey]", "--output", "text",
            ],
            progress,
        )?;
        let mut parts = text.split_whitespace();
        match (parts.next(), parts.next()) {
            (Some(id), Some(secret)) => Ok((id.to_string(), Secret::new(secret))),
            _ => Err(IamError::Remote("발급한 키를 읽지 못했습니다".into())),
        }
    }

    /// 새 키는 AWS 에 퍼지기까지 몇 초 걸린다. 그 사이의 거절은 기다렸다 다시 묻는다.
    fn identify(&self, key_id: &str, secret: &Secret, progress: &dyn ProgressSink) -> Result<String, IamError> {
        let mut last = IamError::Remote("들어가 보지 못했습니다".into());
        for attempt in 1..=PROPAGATION_TRIES {
            match self.identify_once(key_id, secret, &Waiting(progress)) {
                Ok(arn) => return Ok(arn),
                Err(e) => {
                    let pending = matches!(&e, IamError::Remote(d)
                        if d.contains("InvalidClientTokenId") || d.contains("SignatureDoesNotMatch"));
                    if !pending {
                        return Err(e);
                    }
                    last = e;
                }
            }
            if attempt < PROPAGATION_TRIES {
                progress.line(Channel::Out, &format!("새 키가 퍼지기를 기다립니다 ({attempt}/{PROPAGATION_TRIES})"));
                std::thread::sleep(PROPAGATION_WAIT);
            }
        }
        Err(last)
    }

    fn delete_user(&self, master: &str, name: &str, progress: &dyn ProgressSink) -> Result<(), IamError> {
        let keys = match self.keys_of(master, name, progress) {
            Err(e) if already_gone(&e) => return Ok(()),
            other => other?,
        };
        for key in keys {
            self.delete_key(master, name, &key, progress)?;
        }
        for policy in self.inline_policies(master, name, progress)? {
            self.run(
                master,
                &["iam", "delete-user-policy", "--user-name", name, "--policy-name", &policy],
                progress,
            )?;
        }
        for arn in self.attached_policies(master, name, progress)? {
            self.run(
                master,
                &["iam", "detach-user-policy", "--user-name", name, "--policy-arn", &arn],
                progress,
            )?;
        }
        self.run(master, &["iam", "delete-user", "--user-name", name], progress)
            .map(|_| ())
    }

    fn last_used(&self, master: &str, key_id: &str, progress: &dyn ProgressSink) -> Result<Option<LastUse>, IamError> {
        let text = self.run(
            master,
            &[
                "iam", "get-access-key-last-used", "--access-key-id", key_id,
                "--query", "AccessKeyLastUsed.[LastUsedDate,ServiceName,Region]", "--output", "text",
            ],
            progress,
        )?;
        let parts: Vec<&str> = text.split_whitespace().collect();
        match parts.as_slice() {
            [at, service, region] if *at != "None" => Ok(Some(LastUse {
                at: at.to_string(),
                service: service.to_string(),
                region: region.to_string(),
            })),
            _ => Ok(None),
        }
    }

    fn user_names(&self, master: &str, progress: &dyn ProgressSink) -> Result<Vec<String>, IamError> {
        let text = self.run(
            master,
            &["iam", "list-users", "--query", "Users[].UserName", "--output", "text"],
            progress,
        )?;
        let mut names = words(&text);
        names.sort();
        Ok(names)
    }

    fn caller_name(&self, master: &str, progress: &dyn ProgressSink) -> Result<String, IamError> {
        let text = self.run(
            master,
            &["sts", "get-caller-identity", "--query", "Arn", "--output", "text"],
            progress,
        )?;
        user_of_arn(&text).ok_or_else(|| IamError::Remote(format!("마스터 계정이 IAM 사용자가 아닙니다: {}", text.trim())))
    }

    /// 정책 원문은 JSON 으로 받는다. text 출력은 문서를 한 줄로 뭉개 읽을 수 없다.
    fn describe_user(&self, master: &str, name: &str, progress: &dyn ProgressSink) -> Result<ExistingUser, IamError> {
        let created = self.run(
            master,
            &["iam", "get-user", "--user-name", name, "--query", "User.CreateDate", "--output", "text"],
            progress,
        )?;
        let keys = self.run(
            master,
            &[
                "iam", "list-access-keys", "--user-name", name,
                "--query", "AccessKeyMetadata[].[AccessKeyId,CreateDate]", "--output", "text",
            ],
            progress,
        )?;
        let mut inline_policies = Vec::new();
        for policy in self.inline_policies(master, name, progress)? {
            inline_policies.push(self.run(
                master,
                &[
                    "iam", "get-user-policy", "--user-name", name, "--policy-name", &policy,
                    "--query", "PolicyDocument", "--output", "json",
                ],
                progress,
            )?);
        }
        Ok(ExistingUser {
            created_at: value(&created).ok_or_else(|| IamError::Remote("만든 시각을 읽지 못했습니다".into()))?,
            keys: key_rows(&keys),
            inline_policies,
            managed_policies: self.attached_policies(master, name, progress)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_user_arn_gives_its_last_path_segment() {
        assert_eq!(user_of_arn("arn:aws:iam::123:user/david-lee-admin\n").as_deref(), Some("david-lee-admin"));
        assert_eq!(user_of_arn("arn:aws:iam::123:user/team/ops/bot").as_deref(), Some("bot"));
        assert_eq!(user_of_arn("arn:aws:iam::123:root"), None);
        assert_eq!(user_of_arn("arn:aws:sts::123:assumed-role/r/i-1"), None);
    }

    #[test]
    fn key_rows_read_id_and_creation_per_line() {
        let rows = key_rows("AKIA1\t2026-04-29T08:24:06+00:00\nAKIA2\t2026-05-01T00:00:00+00:00\n\n");
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[1].id.as_str(), rows[1].created_at.as_str()), ("AKIA2", "2026-05-01T00:00:00+00:00"));
    }
}
