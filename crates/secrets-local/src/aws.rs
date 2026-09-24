//! AWS 에 묻는 일.
//!
//! 그 계정의 격리 홈으로 `aws` 를 돌린다. 지금 전역으로 무엇이 활성이든 상관없이,
//! 사용자가 화면에서 고른 계정으로 나간다.
//!
//! 응답은 `--query` 와 `--output text` 로 받는다. JSON 라이브러리를 들이지 않으려는
//! 것이고, 무엇을 꺼내 오는지가 명령에 그대로 적혀 사람이 읽을 수 있다.

use secrets_core::account::Provider;
use secrets_core::aws::{AwsError, AwsGateway, Machine, SeenKeyPair};
use secrets_core::port::{Channel, ProgressSink};

use crate::cli::{exec, tools};
use crate::vault::paths;

pub struct CliAws;

fn remote(e: impl std::fmt::Display) -> AwsError {
    AwsError::Remote(e.to_string())
}

/// `aws` 를 그 계정의 홈으로 돌리고 stdout 을 모은다.
///
/// stdout 과 stderr 를 섞지 않는다. 응답을 쪼개야 하는데 경고 한 줄이 끼면 줄이 밀린다.
pub(crate) fn aws(account: &str, args: &[&str], progress: &dyn ProgressSink) -> Result<String, AwsError> {
    let home = paths::cli_home_of(Provider::Aws, account);
    aws_with(&paths::env_for(Provider::Aws, &home), args, progress)
}

/// 주어진 환경으로 `aws` 를 돌린다. 계정 홈이 아닌 자격으로 나갈 때 쓴다.
///
/// stdout 은 진행 창에 흘리지 않는다. 시크릿이 응답으로 오는 명령이 있다.
pub(crate) fn aws_with(
    env: &[(&'static str, String)],
    args: &[&str],
    progress: &dyn ProgressSink,
) -> Result<String, AwsError> {
    let program = tools::find_in_path("aws")
        .ok_or_else(|| AwsError::Remote("aws 를 찾을 수 없습니다".into()))?;

    progress.line(Channel::Out, &format!("$ {}", exec::display("aws", args)));

    let buffer = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = buffer.clone();
    // 실패했을 때 사람에게 보여 줄 말은 AWS 가 이미 하고 있다. 종료 코드만
    // 전하면 그 말을 버리는 셈이라, 흘려보내면서 같이 모아 둔다.
    let trouble = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let noted = trouble.clone();

    let outcome = exec::run_full(&program, args, env, None, move |stream, line| match stream {
        exec::Stream::Stdout => {
            let mut held = sink.lock().unwrap();
            held.push_str(&line);
            held.push('\n');
        }
        exec::Stream::Stderr => {
            progress.line(Channel::Err, &line);
            *noted.lock().unwrap() = line;
        }
    })
    .map_err(remote)?;

    let text = buffer.lock().unwrap().clone();
    if !outcome.ok() {
        let said = trouble.lock().unwrap().clone();
        return Err(AwsError::Remote(said_or_code(&said, outcome.code)));
    }
    Ok(text)
}

impl CliAws {
    fn ec2_key_pairs(
        &self,
        account: &str,
        region: &str,
        progress: &dyn ProgressSink,
    ) -> Result<Vec<SeenKeyPair>, AwsError> {
        let text = aws(
            account,
            &[
                "ec2",
                "describe-key-pairs",
                "--region",
                region,
                "--query",
                "KeyPairs[].[KeyName,KeyFingerprint]",
                "--output",
                "text",
            ],
            progress,
        )?;
        Ok(key_pair_rows(&text))
    }

    /// 기본 키페어는 `--include-default-key-pair` 를 주어야 목록에 나온다.
    fn lightsail_key_pairs(
        &self,
        account: &str,
        region: &str,
        progress: &dyn ProgressSink,
    ) -> Result<Vec<SeenKeyPair>, AwsError> {
        let text = aws(
            account,
            &[
                "lightsail",
                "get-key-pairs",
                "--include-default-key-pair",
                "--region",
                region,
                "--query",
                "keyPairs[].[name,fingerprint]",
                "--output",
                "text",
            ],
            progress,
        )?;
        Ok(key_pair_rows(&text))
    }
}

/// `이름<TAB>지문` 줄들. 지문 칸이 `None` 이면 AWS 가 주지 않은 것이다.
fn key_pair_rows(text: &str) -> Vec<SeenKeyPair> {
    text.lines()
        .filter_map(|line| {
            let mut cells = line.split('\t');
            let name = cells.next()?.trim();
            if name.is_empty() {
                return None;
            }
            Some(SeenKeyPair {
                name: name.to_string(),
                fingerprint: cells.next().and_then(value),
            })
        })
        .collect()
}

/// AWS 가 남긴 말. 없으면 종료 코드라도 전한다.
///
/// `aws: [ERROR]: An error occurred (…) when calling …` 에서 앞의 장식을 걷어낸다.
pub(crate) fn said_or_code(said: &str, code: Option<i32>) -> String {
    let trimmed = said
        .trim()
        .trim_start_matches("aws: ")
        .trim_start_matches("[ERROR]: ")
        .trim();

    if trimmed.is_empty() {
        return format!("aws 가 실패했습니다 ({})", code.unwrap_or(-1));
    }
    trimmed.to_string()
}

pub(crate) fn value(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed == "None" {
        return None;
    }
    Some(trimmed.to_string())
}

impl AwsGateway for CliAws {
    fn account_id(&self, account: &str, progress: &dyn ProgressSink) -> Result<String, AwsError> {
        let text = aws(
            account,
            &["sts", "get-caller-identity", "--query", "Account", "--output", "text"],
            progress,
        )?;
        value(&text).ok_or_else(|| AwsError::Remote("계정 ID 를 읽지 못했습니다".into()))
    }

    fn key_pairs(
        &self,
        account: &str,
        machine: Machine,
        region: &str,
        progress: &dyn ProgressSink,
    ) -> Result<Vec<SeenKeyPair>, AwsError> {
        match machine {
            Machine::Ec2 => self.ec2_key_pairs(account, region, progress),
            Machine::Lightsail => self.lightsail_key_pairs(account, region, progress),
        }
    }
}

/// 이 맥에 있는 개인 키 중 그 지문에 맞는 것.
///
/// 키페어를 금고로 들이려면 개인 키가 있어야 하는데, AWS 는 그것을 다시 주지 않는다.
/// 그러니 이미 손에 있는지부터 봐야 한다. `~/.ssh` 아래를 전부 훑는다 — `.pem` 만
/// 보면 확장자 없이 둔 키를 놓친다.
pub fn local_match(fingerprint: &str) -> Option<std::path::PathBuf> {
    use secrets_core::aws::fingerprint as fp;

    fp::normalize(fingerprint)?;
    let root = std::path::PathBuf::from(std::env::var_os("HOME")?).join(".ssh");
    candidates(&root)
        .into_iter()
        .find(|path| fingerprint_of(path).is_some_and(|mine| fp::same(&mine, fingerprint)))
}

fn candidates(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(candidates(&path));
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        // 공개 키·설정·알려진 호스트는 개인 키가 아니다.
        if name.ends_with(".pub") || name.starts_with("config") || name.starts_with("known_hosts") {
            continue;
        }
        found.push(path);
    }
    found.sort();
    found
}

fn fingerprint_of(path: &std::path::Path) -> Option<String> {
    let program = tools::find_in_path("ssh-keygen")?;
    let buffer = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = buffer.clone();

    let outcome = exec::run(
        &program,
        &["-l", "-f", &path.display().to_string()],
        move |stream, line| {
            if stream == exec::Stream::Stdout {
                sink.lock().unwrap().push_str(&line);
            }
        },
    )
    .ok()?;
    if !outcome.ok() {
        return None;
    }

    let text = buffer.lock().unwrap().clone();
    text.split_whitespace()
        .find(|part| part.starts_with("SHA256:"))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_pair_rows_read_names_and_fingerprints_and_a_missing_one_as_none() {
        let text = "tuk-key\tgzsyNYxNoWmZrAdAQCtVPqaIoTmXn6cHnHfE4XSZjsM=\nLightsailDefaultKeyPair\tNone\n\n";
        let rows = key_pair_rows(text);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "tuk-key");
        assert!(rows[0].fingerprint.is_some());
        assert_eq!(rows[1].name, "LightsailDefaultKeyPair");
        assert_eq!(rows[1].fingerprint, None);
    }
}
