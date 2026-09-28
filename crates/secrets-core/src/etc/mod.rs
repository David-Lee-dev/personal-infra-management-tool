//! 기타 — 다시 받을 수 없는 파일과 그 파일을 여는 값.
//!
//! 금고가 만들 수 없는 것이라 들이기만 한다. 지우는 길은 없다 — 걷어내면 보관소로 간다.
//! 소비처는 기록만 한다. 빌드 설정 같은 소비처의 파일은 사람이 고친다.
//!
//! ```text
//! keys/etc/<프로젝트>/<이름>/
//!   item.toml     기록 — 종류 · 용도 · 해시 · 여는 값의 이름 · 소비처
//!   files/        들인 파일 그대로
//!   values.env    그 파일을 여는 값
//! ```

use serde::{Deserialize, Serialize};

use crate::credential::secret::Secret;
use crate::port::Clock;

/// 이 맥을 가리키는 소비처 호스트.
pub const LOCAL_HOST: &str = "local";

/// 기타 항목 하나의 자리.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EtcRef {
    pub project: String,
    pub name: String,
}

impl EtcRef {
    pub fn slug(&self) -> String {
        format!("{}/{}", self.project, self.name)
    }
}

/// 이 파일의 사본이 놓인 곳. 사람이 적는다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EtcConsumer {
    pub host: String,
    pub file: String,
    pub recorded_at: String,
}

/// `item.toml` 의 모양. 값 자체는 여기 없고 이름만 있다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtcItem {
    pub project: String,
    pub name: String,
    /// android | apple | service | file
    pub kind: String,
    #[serde(default)]
    pub purpose: String,
    /// `files/` 아래 파일 이름.
    pub file: String,
    pub size: u64,
    pub sha256: String,
    pub adopted_at: String,
    /// `values.env` 에 있는 값의 이름.
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default)]
    pub consumers: Vec<EtcConsumer>,
    /// 만료일 `YYYY-MM-DD` 또는 `never`. 사람이 적는다. 모르면 없다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
}

impl EtcItem {
    pub fn at(&self) -> EtcRef {
        EtcRef {
            project: self.project.clone(),
            name: self.name.clone(),
        }
    }
}

#[derive(Debug)]
pub enum EtcError {
    /// 사람이 적은 값이 쓸 수 없는 모양이다.
    Invalid(String),
    Missing(String),
    /// 이미 기록된 자리다.
    Taken(String),
    Storage(String),
}

impl std::fmt::Display for EtcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EtcError::Invalid(detail) => write!(f, "{detail}"),
            EtcError::Missing(what) => write!(f, "{what}을(를) 찾을 수 없습니다."),
            EtcError::Taken(what) => write!(f, "{what}은(는) 이미 기록되어 있습니다."),
            EtcError::Storage(detail) => write!(f, "{detail}"),
        }
    }
}

/// 담을 수 있는 종류. `item.toml` 의 `kind` 에 이 id 가 적힌다.
pub const KINDS: [&str; 5] = ["android", "apple", "apple-ads", "service", "file"];

/// 들일 것 — 사람이 적은 자리 · 종류 · 용도, 원래 파일, 그 파일을 여는 값.
pub struct EtcAdoption {
    pub at: EtcRef,
    pub kind: String,
    pub purpose: String,
    /// 원래 파일의 절대 경로. 들이면 금고로 옮겨지고 여기서는 사라진다.
    pub source: String,
    pub values: Vec<(String, Secret)>,
}

/// 기타 항목이 놓이는 곳.
pub trait EtcVault: Send + Sync {
    /// 전부. 읽지 못한 기록은 건너뛰지 않고 오류로 돌려준다.
    fn list(&self) -> Vec<Result<EtcItem, String>>;
    fn load(&self, at: &EtcRef) -> Result<EtcItem, EtcError>;
    /// 기록을 통째로 다시 쓴다. 파일과 값은 건드리지 않는다.
    fn record(&self, item: &EtcItem) -> Result<(), EtcError>;
    /// `values.env` 의 값 하나.
    fn value(&self, at: &EtcRef, name: &str) -> Result<Secret, EtcError>;
    /// 원래 파일을 `files/` 로 옮기고 값과 기록을 쓴다. 금고의 사본이 원본과 같을 때만 원본을 지운다.
    /// 크기와 해시는 여기서 잰다.
    fn adopt(&self, adoption: &EtcAdoption, adopted_at: &str) -> Result<EtcItem, EtcError>;
}

/// 자리 한 조각(프로젝트 · 이름). 디렉토리 이름이 되므로 경로로 안전한 글자만 받는다.
fn check_segment(label: &str, text: &str) -> Result<String, EtcError> {
    let segment = text.trim();
    let valid = !segment.is_empty()
        && segment.len() <= 64
        && !segment.starts_with('.')
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if valid {
        Ok(segment.to_string())
    } else {
        Err(EtcError::Invalid(format!(
            "{label}에는 영문 · 숫자 · - · _ · .만 쓸 수 있고, .으로 시작할 수 없습니다."
        )))
    }
}

/// 여는 값의 이름과 값. `values.env` 의 한 줄이 되므로 이름에 `=` · 공백을, 값에 줄바꿈을 받지 않는다.
fn check_values(values: &[(String, Secret)]) -> Result<Vec<(String, Secret)>, EtcError> {
    let mut checked: Vec<(String, Secret)> = Vec::new();
    for (name, value) in values {
        let name = name.trim();
        let valid_name = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
        if !valid_name {
            return Err(EtcError::Invalid(format!(
                "값 이름 '{name}'에는 영문 · 숫자 · _ · - · .만 쓸 수 있습니다."
            )));
        }
        if checked.iter().any(|(n, _)| n == name) {
            return Err(EtcError::Invalid(format!("값 이름 {name}이(가) 두 번 있습니다.")));
        }
        if value.is_empty() || value.expose().contains(['\n', '\r']) {
            return Err(EtcError::Invalid(format!(
                "{name}의 값을 한 줄로 입력하세요."
            )));
        }
        checked.push((name.to_string(), Secret::new(value.expose().trim())));
    }
    Ok(checked)
}

/// 기록을 고치는 일. 파일과 값은 들일 때 정해지고, 여기서는 사람이 적는 것만 바꾼다.
pub struct EtcBook<'a> {
    vault: &'a dyn EtcVault,
    clock: &'a dyn Clock,
}

impl<'a> EtcBook<'a> {
    pub fn new(vault: &'a dyn EtcVault, clock: &'a dyn Clock) -> EtcBook<'a> {
        EtcBook { vault, clock }
    }

    /// 파일 하나와 그 파일을 여는 값을 들인다. 원래 파일은 금고로 옮겨진다.
    pub fn adopt(&self, adoption: EtcAdoption) -> Result<EtcItem, EtcError> {
        let at = EtcRef {
            project: check_segment("그룹", &adoption.at.project)?,
            name: check_segment("이름", &adoption.at.name)?,
        };
        if !KINDS.contains(&adoption.kind.as_str()) {
            return Err(EtcError::Invalid(format!("종류 {}을(를) 알 수 없습니다.", adoption.kind)));
        }
        let source = adoption.source.trim();
        if source.is_empty() {
            return Err(EtcError::Invalid("들일 파일을 고르세요.".into()));
        }
        if self.vault.load(&at).is_ok() {
            return Err(EtcError::Taken(at.slug()));
        }
        let values = check_values(&adoption.values)?;
        let checked = EtcAdoption {
            at,
            kind: adoption.kind,
            purpose: adoption.purpose.trim().to_string(),
            source: source.to_string(),
            values,
        };
        self.vault.adopt(&checked, &self.clock.now())
    }

    /// 사본이 놓인 곳을 기록한다. 그 파일은 건드리지 않는다.
    pub fn add_consumer(&self, at: &EtcRef, host: &str, file: &str) -> Result<EtcItem, EtcError> {
        let (host, file) = (host.trim(), file.trim());
        if host.is_empty() || file.is_empty() {
            return Err(EtcError::Invalid("호스트와 파일을 적으세요".into()));
        }
        let mut item = self.vault.load(at)?;
        if item.consumers.iter().any(|c| c.host == host && c.file == file) {
            return Err(EtcError::Taken(format!("{host}:{file}")));
        }
        item.consumers.push(EtcConsumer {
            host: host.to_string(),
            file: file.to_string(),
            recorded_at: self.clock.now(),
        });
        self.vault.record(&item)?;
        Ok(item)
    }

    /// 기록에서 뺀다. 그 파일은 건드리지 않는다.
    pub fn remove_consumer(&self, at: &EtcRef, host: &str, file: &str) -> Result<EtcItem, EtcError> {
        let mut item = self.vault.load(at)?;
        let index = item
            .consumers
            .iter()
            .position(|c| c.host == host && c.file == file)
            .ok_or_else(|| EtcError::Missing(format!("{host}:{file}")))?;
        item.consumers.remove(index);
        self.vault.record(&item)?;
        Ok(item)
    }

    /// 만료일을 적는다. 빈 값은 지운다.
    pub fn set_expires(&self, at: &EtcRef, to: &str) -> Result<EtcItem, EtcError> {
        let expires = crate::expiry::check_expires(to).map_err(EtcError::Invalid)?;
        let mut item = self.vault.load(at)?;
        item.expires = expires;
        self.vault.record(&item)?;
        Ok(item)
    }

    pub fn set_purpose(&self, at: &EtcRef, to: &str) -> Result<EtcItem, EtcError> {
        let mut item = self.vault.load(at)?;
        item.purpose = to.trim().to_string();
        self.vault.record(&item)?;
        Ok(item)
    }

    /// 여는 값 하나. 기록에 이름이 없는 값은 꺼내지 않는다.
    pub fn value(&self, at: &EtcRef, name: &str) -> Result<Secret, EtcError> {
        let item = self.vault.load(at)?;
        if !item.values.iter().any(|v| v == name) {
            return Err(EtcError::Missing(format!("{} 의 값 {name}", at.slug())));
        }
        self.vault.value(at, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct Vault {
        items: Mutex<HashMap<String, EtcItem>>,
        values: HashMap<String, String>,
    }

    impl Vault {
        fn with(item: EtcItem) -> Vault {
            let mut items = HashMap::new();
            items.insert(item.at().slug(), item);
            let mut values = HashMap::new();
            values.insert("storePassword".to_string(), "s3cret".to_string());
            // 기록(item.values)에는 없는 값. 기록이 막는지 보려고 값 파일에만 둔다.
            values.insert("keyPassword".to_string(), "hidden".to_string());
            Vault {
                items: Mutex::new(items),
                values,
            }
        }

        fn saved(&self, at: &EtcRef) -> EtcItem {
            self.items.lock().unwrap()[&at.slug()].clone()
        }
    }

    impl EtcVault for Vault {
        fn list(&self) -> Vec<Result<EtcItem, String>> {
            self.items.lock().unwrap().values().cloned().map(Ok).collect()
        }
        fn load(&self, at: &EtcRef) -> Result<EtcItem, EtcError> {
            self.items
                .lock()
                .unwrap()
                .get(&at.slug())
                .cloned()
                .ok_or_else(|| EtcError::Missing(at.slug()))
        }
        fn record(&self, item: &EtcItem) -> Result<(), EtcError> {
            self.items.lock().unwrap().insert(item.at().slug(), item.clone());
            Ok(())
        }
        fn value(&self, _at: &EtcRef, name: &str) -> Result<Secret, EtcError> {
            self.values
                .get(name)
                .map(|v| Secret::new(v.clone()))
                .ok_or_else(|| EtcError::Missing(name.to_string()))
        }
        fn adopt(&self, adoption: &EtcAdoption, adopted_at: &str) -> Result<EtcItem, EtcError> {
            let item = EtcItem {
                project: adoption.at.project.clone(),
                name: adoption.at.name.clone(),
                kind: adoption.kind.clone(),
                purpose: adoption.purpose.clone(),
                file: adoption.source.rsplit('/').next().unwrap_or_default().to_string(),
                size: 0,
                sha256: String::new(),
                adopted_at: adopted_at.to_string(),
                values: adoption.values.iter().map(|(n, _)| n.clone()).collect(),
                consumers: Vec::new(),
                expires: None,
            };
            self.items.lock().unwrap().insert(item.at().slug(), item.clone());
            Ok(item)
        }
    }

    struct Clock;
    impl crate::port::Clock for Clock {
        fn now(&self) -> String {
            "2026-09-24T16:00:00+09:00".into()
        }
        fn today(&self) -> String {
            "2026-09-24".into()
        }
    }

    fn item() -> EtcItem {
        EtcItem {
            project: "tuk-app".into(),
            name: "android-upload".into(),
            kind: "android".into(),
            purpose: String::new(),
            file: "upload-keystore.jks".into(),
            size: 2744,
            sha256: "d7ed".into(),
            adopted_at: "2026-09-23T18:15:26+09:00".into(),
            values: vec!["storePassword".into(), "keyAlias".into()],
            consumers: vec![EtcConsumer {
                host: LOCAL_HOST.into(),
                file: "~/app/android/key.properties".into(),
                recorded_at: "t".into(),
            }],
            expires: None,
        }
    }

    fn at() -> EtcRef {
        item().at()
    }

    #[test]
    fn a_consumer_is_recorded_trimmed_with_the_time() {
        let vault = Vault::with(item());
        let book = EtcBook::new(&vault, &Clock);
        book.add_consumer(&at(), " tukapp-prod ", " /home/deploy/secrets/key ").unwrap();

        let last = vault.saved(&at()).consumers.pop().unwrap();
        assert_eq!(last.host, "tukapp-prod");
        assert_eq!(last.file, "/home/deploy/secrets/key");
        assert_eq!(last.recorded_at, "2026-09-24T16:00:00+09:00");
    }

    #[test]
    fn an_empty_host_or_file_is_refused_and_nothing_is_written() {
        let vault = Vault::with(item());
        let book = EtcBook::new(&vault, &Clock);
        for (host, file) in [("", "/x"), ("h", "  "), (" ", "")] {
            assert!(matches!(book.add_consumer(&at(), host, file), Err(EtcError::Invalid(_))));
        }
        assert_eq!(vault.saved(&at()).consumers.len(), 1);
    }

    #[test]
    fn the_same_place_twice_is_refused() {
        let vault = Vault::with(item());
        let book = EtcBook::new(&vault, &Clock);
        let taken = book.add_consumer(&at(), "local", "~/app/android/key.properties");
        assert!(matches!(taken, Err(EtcError::Taken(_))));
        assert_eq!(vault.saved(&at()).consumers.len(), 1);
    }

    #[test]
    fn removing_takes_out_only_that_place() {
        let vault = Vault::with(item());
        let book = EtcBook::new(&vault, &Clock);
        book.add_consumer(&at(), "tukapp-dev", "/home/deploy/secrets/key").unwrap();
        book.remove_consumer(&at(), "local", "~/app/android/key.properties").unwrap();

        let left = vault.saved(&at()).consumers;
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].host, "tukapp-dev");
    }

    #[test]
    fn removing_an_unrecorded_place_says_missing() {
        let vault = Vault::with(item());
        let book = EtcBook::new(&vault, &Clock);
        let missing = book.remove_consumer(&at(), "tukapp-prod", "/nowhere");
        assert!(matches!(missing, Err(EtcError::Missing(_))));
    }

    #[test]
    fn an_expiry_date_is_checked_before_it_is_written() {
        let vault = Vault::with(item());
        let book = EtcBook::new(&vault, &Clock);
        book.set_expires(&at(), "2027-09-01").unwrap();
        assert_eq!(vault.saved(&at()).expires.as_deref(), Some("2027-09-01"));
        assert!(matches!(book.set_expires(&at(), "9월"), Err(EtcError::Invalid(_))));
        assert_eq!(vault.saved(&at()).expires.as_deref(), Some("2027-09-01"));
    }

    #[test]
    fn purpose_is_trimmed() {
        let vault = Vault::with(item());
        EtcBook::new(&vault, &Clock).set_purpose(&at(), "  Play 스토어 업로드 ").unwrap();
        assert_eq!(vault.saved(&at()).purpose, "Play 스토어 업로드");
    }

    #[test]
    fn a_value_is_given_only_when_its_name_is_on_the_record() {
        let vault = Vault::with(item());
        let book = EtcBook::new(&vault, &Clock);
        assert_eq!(book.value(&at(), "storePassword").unwrap().expose(), "s3cret");
        // 값 파일에 있어도 기록에 없는 이름은 꺼내지 않는다.
        assert!(matches!(book.value(&at(), "keyPassword"), Err(EtcError::Missing(_))));
    }

    fn adoption(project: &str, name: &str, values: &[(&str, &str)]) -> EtcAdoption {
        EtcAdoption {
            at: EtcRef {
                project: project.into(),
                name: name.into(),
            },
            kind: "apple-ads".into(),
            purpose: "  캠페인 API ".into(),
            source: "/Users/me/Downloads/private-key.pem".into(),
            values: values.iter().map(|(n, v)| (n.to_string(), Secret::new(*v))).collect(),
        }
    }

    #[test]
    fn adopting_records_the_names_of_the_values_trimmed_with_the_time() {
        let vault = Vault::with(item());
        let book = EtcBook::new(&vault, &Clock);
        let adopted = book
            .adopt(adoption(" nemo ", "apple-ads", &[(" clientId ", " SEARCHADS.abc "), ("teamId", "SEARCHADS.abc")]))
            .unwrap();

        assert_eq!(adopted.at().slug(), "nemo/apple-ads");
        assert_eq!(adopted.values, vec!["clientId", "teamId"]);
        assert_eq!(adopted.purpose, "캠페인 API");
        assert_eq!(adopted.adopted_at, "2026-09-24T16:00:00+09:00");
    }

    #[test]
    fn adopting_into_a_taken_place_is_refused() {
        let vault = Vault::with(item());
        let book = EtcBook::new(&vault, &Clock);
        let taken = book.adopt(adoption("tuk-app", "android-upload", &[]));
        assert!(matches!(taken, Err(EtcError::Taken(_))));
    }

    #[test]
    fn adopting_refuses_unsafe_places_kinds_and_values() {
        let vault = Vault::with(item());
        let book = EtcBook::new(&vault, &Clock);
        for (project, name) in [("", "x"), ("../up", "x"), ("nemo", ".hidden"), ("nemo", "a/b")] {
            let refused = book.adopt(adoption(project, name, &[]));
            assert!(matches!(refused, Err(EtcError::Invalid(_))), "{project}/{name}");
        }
        let mut unknown = adoption("nemo", "x", &[]);
        unknown.kind = "password".into();
        assert!(matches!(book.adopt(unknown), Err(EtcError::Invalid(_))));
        let mut no_file = adoption("nemo", "x", &[]);
        no_file.source = "  ".into();
        assert!(matches!(book.adopt(no_file), Err(EtcError::Invalid(_))));
        for values in [
            vec![("key id", "1")],
            vec![("a=b", "1")],
            vec![("keyId", "")],
            vec![("keyId", "1\n2")],
            vec![("keyId", "1"), ("keyId", "2")],
        ] {
            let refused = book.adopt(adoption("nemo", "x", &values));
            assert!(matches!(refused, Err(EtcError::Invalid(_))), "{:?}", values.iter().map(|(n, _)| n).collect::<Vec<_>>());
        }
        assert!(vault.load(&EtcRef { project: "nemo".into(), name: "x".into() }).is_err());
    }
}
