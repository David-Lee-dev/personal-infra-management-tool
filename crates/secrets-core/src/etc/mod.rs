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
            EtcError::Missing(what) => write!(f, "{what} 이(가) 없습니다"),
            EtcError::Taken(what) => write!(f, "{what} 은(는) 이미 기록돼 있습니다"),
            EtcError::Storage(detail) => write!(f, "{detail}"),
        }
    }
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
}
