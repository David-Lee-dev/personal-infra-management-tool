//! IAM 정책 읽기.
//!
//! 정책은 사람이 친 JSON 그대로 받는다. 꼴을 미리 정해 두면 그 밖의 권한을 줄 수
//! 없다. 대신 여기서 읽어 **무엇을 허용하는지**와 **규칙에서 어디가 벗어났는지**를
//! 돌려준다. 벗어난 것은 알리기만 한다 — 판단은 사람이 한다.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Allow,
    Deny,
}

/// 정책 문장 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    pub effect: Effect,
    pub actions: Vec<String>,
    pub resources: Vec<String>,
    /// 조건 연산자 이름들. 값은 보여 주지 않는다 — 길고, 연산자로 성격이 드러난다.
    pub conditions: Vec<String>,
    /// `NotAction` · `NotResource` 로 범위를 뒤집었는가.
    pub inverted: bool,
}

/// 읽은 정책. 원문도 같이 든다 — AWS 에는 원문이 간다.
#[derive(Debug, Clone)]
pub struct Policy {
    pub text: String,
    pub statements: Vec<Statement>,
}

/// 만든 뒤 AWS 에 물어볼 것 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub action: String,
    pub resource: String,
    pub allowed: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PolicyError(pub String);

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn strings(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(one)) => vec![one.clone()],
        Some(Value::Array(many)) => many
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

impl Policy {
    /// JSON 을 읽는다. 문법이 틀렸거나 문장이 없거나, 문장에 동작 · 대상이 없으면 실패한다.
    pub fn read(text: &str) -> Result<Policy, PolicyError> {
        let doc: Value = serde_json::from_str(text)
            .map_err(|e| PolicyError(format!("JSON 이 아닙니다 — {e}")))?;

        let raw = match doc.get("Statement") {
            Some(Value::Array(many)) => many.clone(),
            Some(one @ Value::Object(_)) => vec![one.clone()],
            _ => return Err(PolicyError("Statement 가 없습니다".into())),
        };
        if raw.is_empty() {
            return Err(PolicyError("Statement 가 없습니다".into()));
        }

        let mut statements = Vec::new();
        for (index, st) in raw.iter().enumerate() {
            statements.push(Policy::statement(index + 1, st)?);
        }
        Ok(Policy {
            text: text.to_string(),
            statements,
        })
    }

    fn statement(number: usize, st: &Value) -> Result<Statement, PolicyError> {
        let effect = match st.get("Effect").and_then(Value::as_str) {
            Some("Allow") => Effect::Allow,
            Some("Deny") => Effect::Deny,
            _ => return Err(PolicyError(format!("{number}번: Effect 는 Allow 나 Deny 여야 합니다"))),
        };
        let inverted = st.get("NotAction").is_some() || st.get("NotResource").is_some();
        let actions = strings(st.get("Action").or_else(|| st.get("NotAction")));
        let resources = strings(st.get("Resource").or_else(|| st.get("NotResource")));
        if actions.is_empty() {
            return Err(PolicyError(format!("{number}번: Action 이 없습니다")));
        }
        if resources.is_empty() {
            return Err(PolicyError(format!("{number}번: Resource 가 없습니다")));
        }
        let conditions = st
            .get("Condition")
            .and_then(Value::as_object)
            .map(|ops| ops.keys().cloned().collect())
            .unwrap_or_default();

        Ok(Statement {
            effect,
            actions,
            resources,
            conditions,
            inverted,
        })
    }

    /// 인라인 정책 여러 개를 문장만 모아 하나로 합친다. 들인 IAM 을 한 화면에 보이려고 쓴다.
    ///
    /// 하나라도 읽지 못하면 실패한다. 빠진 정책이 있으면 허용 범위가 실제보다 좁아 보인다.
    pub fn combine(documents: &[String]) -> Result<String, PolicyError> {
        let mut statements = Vec::new();
        for (index, text) in documents.iter().enumerate() {
            let doc: Value = serde_json::from_str(text)
                .map_err(|e| PolicyError(format!("{}번 정책이 JSON 이 아닙니다 — {e}", index + 1)))?;
            match doc.get("Statement") {
                Some(Value::Array(many)) => statements.extend(many.iter().cloned()),
                Some(one @ Value::Object(_)) => statements.push(one.clone()),
                _ => return Err(PolicyError(format!("{}번 정책에 Statement 가 없습니다", index + 1))),
            }
        }
        if statements.is_empty() {
            return Err(PolicyError("정책이 없습니다".into()));
        }
        let combined = serde_json::json!({ "Version": "2012-10-17", "Statement": statements });
        serde_json::to_string_pretty(&combined).map_err(|e| PolicyError(e.to_string()))
    }

    /// 동작이 가리키는 서비스들. 처음 나온 순서대로, 겹치지 않게.
    pub fn services(&self) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        for action in self.statements.iter().flat_map(|st| &st.actions) {
            let service = action.split(':').next().unwrap_or(action).to_string();
            if !found.contains(&service) {
                found.push(service);
            }
        }
        found
    }

    /// 허용 문장의 대상들.
    pub fn resources(&self) -> Vec<&str> {
        self.statements
            .iter()
            .filter(|st| st.effect == Effect::Allow && !st.inverted)
            .flat_map(|st| st.resources.iter().map(String::as_str))
            .collect()
    }

    /// 규칙에서 벗어난 곳. 막지 않고 알린다.
    pub fn problems(&self) -> Vec<String> {
        let mut found = Vec::new();
        for (index, st) in self.statements.iter().enumerate() {
            let at = index + 1;
            if st.inverted {
                found.push(format!("{at}번: NotAction · NotResource 는 범위를 뒤집어 넓힌다"));
            }
            if st.actions.iter().any(|a| a == "*" || a.ends_with(":*")) {
                found.push(format!("{at}번: 동작에 * 가 있다"));
            }
            if st.resources.iter().any(|r| r == "*") {
                found.push(format!("{at}번: 대상이 * 다"));
            }
        }
        let services = self.services();
        if services.len() > 1 {
            found.push(format!(
                "서비스가 {}개다 — {}. IAM 하나에 권한 하나",
                services.len(),
                services.join(" · ")
            ));
        }
        found
    }

    /// 만든 뒤 AWS 에 물어볼 것.
    ///
    /// 허용 문장마다 하나씩 "허용돼야 한다"를 묻고, 다른 서비스의 동작 하나로 "거부돼야
    /// 한다"를 묻는다. 허용만 보면 `*` 정책도 통과한다. 조건이 붙은 문장은 조건
    /// 값 없이 물으면 거부로 나오므로 묻지 않는다. 와일드카드 동작은 이름이 아니라
    /// 물을 수 없다.
    pub fn probes(&self) -> Vec<Probe> {
        let mut asked = Vec::new();
        for st in &self.statements {
            if st.effect != Effect::Allow || st.inverted || !st.conditions.is_empty() {
                continue;
            }
            let Some(action) = st.actions.iter().find(|a| !a.contains('*')) else {
                continue;
            };
            let Some(resource) = st.resources.iter().find(|r| *r != "*") else {
                continue;
            };
            asked.push(Probe {
                action: action.clone(),
                resource: concrete(resource),
                allowed: true,
            });
        }

        let foreign = if self.services().iter().any(|s| s == "iam") {
            "s3:ListAllMyBuckets"
        } else {
            "iam:CreateUser"
        };
        asked.push(Probe {
            action: foreign.into(),
            resource: "*".into(),
            allowed: false,
        });
        asked
    }
}

/// 와일드카드 대상을 그 패턴에 들어맞는 실제 이름 하나로 바꾼다. 시뮬레이터는
/// 대상을 문자열 그대로 패턴에 맞춰 보므로, `*` 를 아무 글자로 채우면 된다.
fn concrete(resource: &str) -> String {
    resource.replace('*', "secrets-probe")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(doc: serde_json::Value) -> Policy {
        Policy::read(&doc.to_string()).unwrap()
    }

    mod read {
        use super::*;

        #[test]
        fn a_single_statement_object_is_read_like_a_list_of_one() {
            let policy = read(serde_json::json!({
                "Statement": { "Effect": "Allow", "Action": "s3:PutObject", "Resource": "arn:aws:s3:::b/*" }
            }));
            assert_eq!(policy.statements.len(), 1);
            assert_eq!(policy.statements[0].actions, vec!["s3:PutObject"]);
        }

        #[test]
        fn broken_json_says_so() {
            let err = Policy::read(r#"{"Statement": ["#).unwrap_err();
            assert!(err.0.starts_with("JSON 이 아닙니다"), "{err}");
        }

        #[test]
        fn a_statement_without_resource_is_refused() {
            let err = Policy::read(
                &serde_json::json!({"Statement": [{"Effect": "Allow", "Action": "s3:PutObject"}]}).to_string(),
            )
            .unwrap_err();
            assert_eq!(err.0, "1번: Resource 가 없습니다");
        }

        #[test]
        fn condition_operators_are_kept_by_name() {
            let policy = read(serde_json::json!({
                "Statement": [{
                    "Effect": "Allow", "Action": "ses:SendEmail", "Resource": "*",
                    "Condition": { "StringEquals": { "ses:FromAddress": "a@b.c" } }
                }]
            }));
            assert_eq!(policy.statements[0].conditions, vec!["StringEquals"]);
        }
    }

    mod combine {
        use super::*;

        #[test]
        fn statements_from_every_document_end_up_in_one_readable_policy() {
            let docs = vec![
                serde_json::json!({"Version": "2012-10-17", "Statement": [
                    {"Sid": "A", "Effect": "Allow", "Action": "s3:PutObject", "Resource": "arn:aws:s3:::b/a/*"},
                    {"Sid": "B", "Effect": "Allow", "Action": "s3:GetObject", "Resource": "arn:aws:s3:::b/b/*"}
                ]})
                .to_string(),
                serde_json::json!({"Statement":
                    {"Effect": "Allow", "Action": "s3:PutObject", "Resource": "arn:aws:s3:::b/c/*"}
                })
                .to_string(),
            ];

            let combined = Policy::read(&Policy::combine(&docs).unwrap()).unwrap();

            let targets: Vec<&str> = combined.resources();
            assert_eq!(targets, vec!["arn:aws:s3:::b/a/*", "arn:aws:s3:::b/b/*", "arn:aws:s3:::b/c/*"]);
        }

        #[test]
        fn one_unreadable_document_fails_the_whole() {
            let docs = vec![
                serde_json::json!({"Statement": [{"Effect": "Allow", "Action": "s3:PutObject", "Resource": "*"}]}).to_string(),
                "{broken".to_string(),
            ];
            assert!(Policy::combine(&docs).is_err());
        }

        #[test]
        fn nothing_to_combine_is_refused() {
            assert!(Policy::combine(&[]).is_err());
        }
    }

    mod problems {
        use super::*;

        #[test]
        fn a_scoped_single_service_policy_has_none() {
            let policy = read(serde_json::json!({
                "Statement": [{ "Effect": "Allow", "Action": ["s3:PutObject", "s3:GetObject"], "Resource": "arn:aws:s3:::b/*" }]
            }));
            assert!(policy.problems().is_empty());
        }

        #[test]
        fn wildcards_and_mixed_services_are_each_named() {
            let policy = read(serde_json::json!({
                "Statement": [
                    { "Effect": "Allow", "Action": "s3:*", "Resource": "*" },
                    { "Effect": "Allow", "Action": "bedrock:InvokeModel", "Resource": "arn:x" }
                ]
            }));
            assert_eq!(
                policy.problems(),
                vec![
                    "1번: 동작에 * 가 있다",
                    "1번: 대상이 * 다",
                    "서비스가 2개다 — s3 · bedrock. IAM 하나에 권한 하나",
                ]
            );
        }

        #[test]
        fn inverted_scope_is_named() {
            let policy = read(serde_json::json!({
                "Statement": [{ "Effect": "Allow", "NotAction": "iam:*", "Resource": "arn:x" }]
            }));
            assert_eq!(policy.problems()[0], "1번: NotAction · NotResource 는 범위를 뒤집어 넓힌다");
        }
    }

    mod probes {
        use super::*;

        #[test]
        fn each_allow_statement_is_asked_once_and_a_foreign_service_must_be_denied() {
            let policy = read(serde_json::json!({
                "Statement": [
                    { "Effect": "Allow", "Action": ["s3:PutObject", "s3:GetObject"], "Resource": "arn:aws:s3:::b/dev/*" },
                    { "Effect": "Allow", "Action": "s3:ListBucket", "Resource": "arn:aws:s3:::b" }
                ]
            }));
            assert_eq!(
                policy.probes(),
                vec![
                    Probe { action: "s3:PutObject".into(), resource: "arn:aws:s3:::b/dev/secrets-probe".into(), allowed: true },
                    Probe { action: "s3:ListBucket".into(), resource: "arn:aws:s3:::b".into(), allowed: true },
                    Probe { action: "iam:CreateUser".into(), resource: "*".into(), allowed: false },
                ]
            );
        }

        #[test]
        fn conditioned_and_wildcard_only_statements_are_not_asked() {
            let policy = read(serde_json::json!({
                "Statement": [
                    { "Effect": "Allow", "Action": "ses:SendEmail", "Resource": "arn:x",
                      "Condition": { "StringEquals": { "ses:FromAddress": "a@b.c" } } },
                    { "Effect": "Allow", "Action": "ses:Send*", "Resource": "arn:x" }
                ]
            }));
            let asked: Vec<bool> = policy.probes().iter().map(|p| p.allowed).collect();
            assert_eq!(asked, vec![false]);
        }

        #[test]
        fn an_iam_policy_is_checked_against_another_service() {
            let policy = read(serde_json::json!({
                "Statement": [{ "Effect": "Allow", "Action": "iam:GetUser", "Resource": "arn:aws:iam::1:user/x" }]
            }));
            assert_eq!(policy.probes().last().unwrap().action, "s3:ListAllMyBuckets");
        }
    }
}
