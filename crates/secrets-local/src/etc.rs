//! 기타 항목이 놓이는 곳 — `keys/etc/<프로젝트>/<이름>/`.
//!
//! 기록(`item.toml`)만 다시 쓴다. 들인 파일(`files/`)과 여는 값(`values.env`)은 읽기만 한다.

use std::path::{Path, PathBuf};

use secrets_core::credential::secret::Secret;
use secrets_core::etc::{EtcError, EtcItem, EtcRef, EtcVault};

use crate::vault;

pub const FILE: &str = "item.toml";
pub const VALUES: &str = "values.env";
pub const FILES: &str = "files";

pub struct FileEtc;

fn storage(e: impl std::fmt::Display) -> EtcError {
    EtcError::Storage(e.to_string())
}

fn root() -> PathBuf {
    vault::root().join("keys").join("etc")
}

pub fn dir_of(at: &EtcRef) -> PathBuf {
    root().join(&at.project).join(&at.name)
}

/// 들인 파일의 자리. 빌드 설정에 적을 절대 경로다.
pub fn file_of(item: &EtcItem) -> PathBuf {
    dir_of(&item.at()).join(FILES).join(&item.file)
}

fn dirs_in(path: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

/// 임시 파일에 다 쓴 뒤 제자리로 옮긴다. 같은 디렉토리 안이라 rename 이 원자적이다.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), EtcError> {
    use std::io::Write;

    let staging = path.with_extension("writing");
    {
        let mut file = std::fs::File::create(&staging).map_err(storage)?;
        file.write_all(bytes).map_err(storage)?;
        file.sync_all().map_err(storage)?;
    }
    vault::restrict(&staging).map_err(storage)?;
    if let Err(e) = std::fs::rename(&staging, path) {
        let _ = std::fs::remove_file(&staging);
        return Err(storage(e));
    }
    Ok(())
}

impl EtcVault for FileEtc {
    fn list(&self) -> Vec<Result<EtcItem, String>> {
        let mut found = Vec::new();
        for project in dirs_in(&root()) {
            for name in dirs_in(&root().join(&project)) {
                let at = EtcRef {
                    project: project.clone(),
                    name,
                };
                if !dir_of(&at).join(FILE).is_file() {
                    continue;
                }
                found.push(self.load(&at).map_err(|e| format!("{} 를 읽지 못했습니다: {e}", at.slug())));
            }
        }
        found
    }

    fn load(&self, at: &EtcRef) -> Result<EtcItem, EtcError> {
        let text = std::fs::read_to_string(dir_of(at).join(FILE))
            .map_err(|_| EtcError::Missing(at.slug()))?;
        toml::from_str(&text).map_err(storage)
    }

    fn record(&self, item: &EtcItem) -> Result<(), EtcError> {
        let dir = dir_of(&item.at());
        if !dir.join(FILE).is_file() {
            // 기록은 들일 때 생긴다. 여기서 새 항목을 만들지 않는다.
            return Err(EtcError::Missing(item.at().slug()));
        }
        let text = toml::to_string_pretty(item).map_err(storage)?;
        write_atomically(&dir.join(FILE), text.as_bytes())
    }

    fn value(&self, at: &EtcRef, name: &str) -> Result<Secret, EtcError> {
        let text = std::fs::read_to_string(dir_of(at).join(VALUES))
            .map_err(|_| EtcError::Missing(format!("{} 의 {VALUES}", at.slug())))?;
        text.lines()
            .filter_map(|line| line.split_once('='))
            .find(|(key, _)| key.trim() == name)
            .map(|(_, value)| Secret::new(value.trim()))
            .ok_or_else(|| EtcError::Missing(format!("{} 의 값 {name}", at.slug())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::tests_support::with_temp_root;

    const RECORD: &str = r#"project = "tuk-app"
name = "android-upload"
kind = "android"
purpose = "Play 스토어 업로드"
file = "upload-keystore.jks"
size = 2744
sha256 = "d7ed"
adopted_at = "2026-09-23T18:15:26+09:00"
values = ["storePassword","keyAlias"]

[[consumers]]
host = "local"
file = "~/app/android/key.properties"
recorded_at = "2026-09-23T18:15:26+09:00"
"#;

    fn place(dir: &Path) -> (EtcRef, PathBuf) {
        let at = EtcRef {
            project: "tuk-app".into(),
            name: "android-upload".into(),
        };
        let here = dir.join("keys/etc/tuk-app/android-upload");
        std::fs::create_dir_all(here.join(FILES)).unwrap();
        std::fs::write(here.join(FILE), RECORD).unwrap();
        std::fs::write(here.join(VALUES), "storePassword=p@ss=word\nkeyAlias=upload\n").unwrap();
        (at, here)
    }

    #[test]
    fn the_record_written_by_adoption_is_read_as_is() {
        with_temp_root(|dir| {
            let (at, _) = place(dir);
            let item = FileEtc.load(&at).unwrap();
            assert_eq!(item.file, "upload-keystore.jks");
            assert_eq!(item.values, vec!["storePassword", "keyAlias"]);
            assert_eq!(item.consumers.len(), 1);
            assert_eq!(FileEtc.list().len(), 1);
        });
    }

    #[test]
    fn a_folder_without_a_record_is_not_an_item() {
        with_temp_root(|dir| {
            place(dir);
            std::fs::create_dir_all(dir.join("keys/etc/tuk/half")).unwrap();
            assert_eq!(FileEtc.list().len(), 1);
        });
    }

    #[test]
    fn rewriting_the_record_keeps_it_private_and_leaves_values_alone() {
        with_temp_root(|dir| {
            let (at, here) = place(dir);
            let mut item = FileEtc.load(&at).unwrap();
            item.purpose = "바뀜".into();
            FileEtc.record(&item).unwrap();

            assert_eq!(FileEtc.load(&at).unwrap().purpose, "바뀜");
            assert_eq!(
                std::fs::read_to_string(here.join(VALUES)).unwrap(),
                "storePassword=p@ss=word\nkeyAlias=upload\n"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(here.join(FILE)).unwrap().permissions().mode();
                assert_eq!(mode & 0o777, 0o600);
            }
        });
    }

    #[test]
    fn recording_does_not_create_an_item_that_was_never_adopted() {
        with_temp_root(|dir| {
            let (at, here) = place(dir);
            let mut item = FileEtc.load(&at).unwrap();
            std::fs::remove_file(here.join(FILE)).unwrap();
            item.purpose = "x".into();
            assert!(matches!(FileEtc.record(&item), Err(EtcError::Missing(_))));
        });
    }

    #[test]
    fn a_value_keeps_everything_after_the_first_equals_sign() {
        with_temp_root(|dir| {
            let (at, _) = place(dir);
            assert_eq!(FileEtc.value(&at, "storePassword").unwrap().expose(), "p@ss=word");
            assert!(matches!(FileEtc.value(&at, "keyPassword"), Err(EtcError::Missing(_))));
        });
    }
}
