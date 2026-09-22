//! CLI 가 설정 홈을 환경변수로 바꿀 수 있는지 실제로 확인한다.
//!
//! 계정별 환경 격리는 이 도구의 기본 동작이지 툴마다 고르는 옵션이 아니다.
//! 그래서 이 모듈은 "이 툴이 격리를 지원하는가" 를 알려주는 정보원이 아니라,
//! 기본 동작이 이 머신에서 실제로 성립하는지 확인하는 자가 점검이다.
//! 성립하지 않는 툴은 쓸 수 없는 툴이며, 배지가 아니라 차단 사유로 다룬다.
//!
//! 프로브는 `~/.secrets/tmp/` 아래 임시 디렉토리만 건드린다. 실제 설정은 읽지도
//! 쓰지도 않는다.

use crate::cli::{exec, tools};
use crate::vault as home;

/// 격리 방식.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mechanism {
    /// 디렉토리 하나를 설정 홈으로 넘긴다.
    ConfigDir(&'static str),
    /// 파일 경로를 각각 넘긴다. aws 처럼 config 와 credentials 가 분리된 경우.
    ConfigFiles(&'static [(&'static str, &'static str)]),
    /// 격리할 설정이 없는 툴. git·ssh·age 처럼 우리가 상태를 맡기지 않는다.
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// 환경변수를 따랐다. 컨텍스트별 격리를 쓸 수 있다.
    Isolated,
    /// 환경변수를 무시하고 원래 설정을 봤다. 다른 수단이 필요하다.
    Leaked,
    /// 프로브 자체가 실패했다. 판정 불가.
    Inconclusive,
    NotApplicable,
}

#[derive(Debug, Clone)]
pub struct Verdict {
    pub status: Status,
    /// 어떤 환경변수를 썼는지 사람이 읽을 수 있게.
    pub mechanism: String,
    /// 그렇게 판정한 근거. 실패했을 때 이게 없으면 원인을 못 찾는다.
    pub evidence: String,
}

/// aws 프로브가 심는 표식 프로필 이름.
const PROBE_PROFILE: &str = "secrets-probe";

/// 툴 하나의 격리 프로브 정의.
struct Spec {
    mechanism: Mechanism,
    /// 프로브 전에 임시 설정 홈을 미리 채운다.
    ///
    /// "기존 계정이 안 보인다" 만으로는 격리와 실패를 구분할 수 없는 툴이 있다.
    /// 그럴 때는 우리가 심은 표식이 보이는지를 대신 확인한다.
    seed: Option<fn(&std::path::Path) -> std::io::Result<()>>,
    /// 격리 여부를 확인하기 위해 실행할 무해한 명령.
    probe_args: &'static [&'static str],
    /// 출력을 보고 "격리됐다" 고 판단하는 규칙.
    ///
    /// 격리가 됐다면 기존 계정이 보이지 않아야 한다. 그래서 대부분
    /// "로그인된 계정이 출력에 없다" 를 확인한다.
    isolated_when: fn(&str) -> bool,
    /// 사람에게 보여줄 판정 기준 설명.
    criterion: &'static str,
}

fn spec(tool_id: &str) -> Option<Spec> {
    Some(match tool_id {
        "gh" => Spec {
            mechanism: Mechanism::ConfigDir("GH_CONFIG_DIR"),
            seed: None,
            probe_args: &["auth", "status"],
            // 격리됐다면 어떤 호스트에도 로그인돼 있지 않다.
            isolated_when: |out| !out.contains("Logged in to"),
            criterion: "auth status 에 로그인된 계정이 없어야 한다",
        },
        "gcloud" => Spec {
            mechanism: Mechanism::ConfigDir("CLOUDSDK_CONFIG"),
            seed: None,
            probe_args: &["config", "list", "--format=value(core.account)"],
            // 계정이 설정돼 있으면 이메일이 찍힌다.
            isolated_when: |out| !out.contains('@'),
            criterion: "core.account 가 비어 있어야 한다",
        },
        "aws" => Spec {
            mechanism: Mechanism::ConfigFiles(&[
                ("AWS_CONFIG_FILE", "config"),
                ("AWS_SHARED_CREDENTIALS_FILE", "credentials"),
            ]),
            // 빈 설정을 가리키면 출력도 없고 파일도 안 생겨서 격리와 실패를
            // 구분할 수 없다. 그래서 표식 프로필을 심어 두고 그게 보이는지 본다.
            seed: Some(|dir| {
                std::fs::write(
                    dir.join("config"),
                    b"[profile secrets-probe]\nregion = us-east-1\n",
                )
            }),
            probe_args: &["configure", "list-profiles"],
            isolated_when: |out| out.contains(PROBE_PROFILE) && !out.contains("default"),
            criterion: "우리가 심은 secrets-probe 프로필만 보여야 한다",
        },
        "firebase" => Spec {
            seed: None,
            // firebase-tools 는 configstore 를 쓴다. XDG_CONFIG_HOME 을 따르는지가
            // 확인되지 않아 이 프로브가 이번 phase 의 실제 목적이다.
            mechanism: Mechanism::ConfigDir("XDG_CONFIG_HOME"),
            probe_args: &["login:list"],
            isolated_when: |out| !out.contains('@'),
            criterion: "로그인된 계정 목록이 비어 있어야 한다",
        },
        _ => return None,
    })
}

/// 툴 하나를 프로브한다. 출력은 `on_line` 으로도 흘려 터미널에 보이게 한다.
pub fn probe<F>(report: &tools::Report, on_line: F) -> Verdict
where
    F: Fn(exec::Stream, String) + Sync,
{
    let Some(spec) = spec(report.tool.id) else {
        return Verdict {
            status: Status::NotApplicable,
            mechanism: "—".into(),
            evidence: "설정 홈을 맡기지 않는 툴".into(),
        };
    };

    let Some(path) = report.path.clone() else {
        return Verdict {
            status: Status::Inconclusive,
            mechanism: describe(&spec.mechanism),
            evidence: "설치되지 않아 확인할 수 없다".into(),
        };
    };

    let scratch = match home::Scratch::new(&format!("probe-{}", report.tool.id)) {
        Ok(s) => s,
        Err(e) => {
            return Verdict {
                status: Status::Inconclusive,
                mechanism: describe(&spec.mechanism),
                evidence: format!("임시 디렉토리를 만들지 못했다: {e}"),
            };
        }
    };

    let env: Vec<(&str, String)> = match spec.mechanism {
        Mechanism::ConfigDir(var) => vec![(var, scratch.path().display().to_string())],
        Mechanism::ConfigFiles(pairs) => pairs
            .iter()
            .map(|(var, name)| (*var, scratch.path().join(name).display().to_string()))
            .collect(),
        Mechanism::NotApplicable => vec![],
    };

    if let Some(seed) = spec.seed
        && let Err(e) = seed(scratch.path())
    {
        return Verdict {
            status: Status::Inconclusive,
            mechanism: describe(&spec.mechanism),
            evidence: format!("프로브용 설정을 준비하지 못했다: {e}"),
        };
    }

    let collected = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = collected.clone();

    let outcome = exec::run_env(&path, spec.probe_args, &env, move |stream, line| {
        if let Ok(mut buf) = sink.lock() {
            buf.push_str(&line);
            buf.push('\n');
        }
        on_line(stream, line);
    });

    let output = collected.lock().map(|b| b.clone()).unwrap_or_default();

    // 거짓 양성 방어. 판정 기준이 대부분 "기존 계정이 안 보인다" 라서,
    // 명령이 아무 일도 못 하고 조용히 죽어도 격리된 것처럼 보인다.
    // 출력도 없고 지정한 디렉토리에 아무것도 안 생겼다면 판정하지 않는다.
    if output.trim().is_empty() && scratch.is_empty() {
        return Verdict {
            status: Status::Inconclusive,
            mechanism: describe(&spec.mechanism),
            evidence: "출력도 없고 지정한 설정 홈에 아무것도 생기지 않았다".into(),
        };
    }

    match outcome {
        // 종료 코드는 판정에 쓰지 않는다. gh auth status 는 로그인이 없으면
        // 1 로 끝나는데, 그건 격리가 됐다는 뜻이지 실패가 아니다.
        Ok(_) if (spec.isolated_when)(&output) => Verdict {
            status: Status::Isolated,
            mechanism: describe(&spec.mechanism),
            evidence: format!("{} — 확인됨", spec.criterion),
        },
        Ok(_) => Verdict {
            status: Status::Leaked,
            mechanism: describe(&spec.mechanism),
            evidence: format!(
                "{} — 그렇지 않았다. 환경변수를 무시하고 기존 설정을 봤다",
                spec.criterion
            ),
        },
        Err(e) => Verdict {
            status: Status::Inconclusive,
            mechanism: describe(&spec.mechanism),
            evidence: format!("프로브 실행 실패: {e}"),
        },
    }
}

/// 프로브에 쓰는 환경변수를 사람이 읽을 수 있는 한 줄로.
pub fn describe(mechanism: &Mechanism) -> String {
    match mechanism {
        Mechanism::ConfigDir(var) => (*var).to_string(),
        Mechanism::ConfigFiles(pairs) => pairs
            .iter()
            .map(|(var, _)| *var)
            .collect::<Vec<_>>()
            .join(" + "),
        Mechanism::NotApplicable => "—".to_string(),
    }
}

/// 이 판정이 툴 사용을 막아야 하는가.
///
/// 격리가 깨진 채로 계정을 여러 개 붙이면 엉뚱한 계정으로 명령이 나간다.
/// 그건 불편이 아니라 사고이므로 차단한다. 판정 불가는 경고로만 둔다 —
/// 프로브의 한계일 수 있어 멀쩡한 툴을 막을 수는 없다.
pub fn blocks(verdict: &Verdict) -> bool {
    verdict.status == Status::Leaked
}

/// 이 툴이 어떤 방식으로 격리되는가. 프로브를 돌리지 않고도 알 수 있는 정보.
pub fn mechanism_of(tool_id: &str) -> Mechanism {
    spec(tool_id)
        .map(|s| s.mechanism)
        .unwrap_or(Mechanism::NotApplicable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_without_spec_are_not_applicable() {
        for id in ["git", "ssh", "age"] {
            assert_eq!(mechanism_of(id), Mechanism::NotApplicable, "{id}");
        }
    }

    #[test]
    fn describes_multi_file_mechanism() {
        assert_eq!(
            describe(&mechanism_of("aws")),
            "AWS_CONFIG_FILE + AWS_SHARED_CREDENTIALS_FILE"
        );
        assert_eq!(describe(&mechanism_of("gh")), "GH_CONFIG_DIR");
    }

    #[test]
    fn aws_criterion_needs_the_seeded_profile() {
        let spec = spec("aws").unwrap();
        // 격리됐다면 우리가 심은 것만 보인다.
        assert!((spec.isolated_when)("secrets-probe\n"));
        // 실제 프로필이 섞여 나오면 누수다.
        assert!(!(spec.isolated_when)(
            "default\ntuk-dev-power\nsecrets-probe\n"
        ));
        // 아무것도 안 나오는 건 격리의 증거가 되지 못한다.
        assert!(!(spec.isolated_when)(""));
    }

    #[test]
    fn gh_criterion_detects_leak() {
        let spec = spec("gh").unwrap();
        assert!((spec.isolated_when)(
            "You are not logged into any GitHub hosts."
        ));
        assert!(!(spec.isolated_when)(
            "  ✓ Logged in to github.com account David-Lee-dev"
        ));
    }
}
