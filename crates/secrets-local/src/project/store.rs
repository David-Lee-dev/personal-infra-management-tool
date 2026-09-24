//! 프로젝트 기록. 기록은 덮어쓰지 않고 새로 쓰기만 한다.

use std::path::{Path, PathBuf};

use secrets_core::project::{ProjectError, ProjectRecord, ProjectStore};

use crate::vault;

pub const FILE: &str = "project.toml";

pub struct FileProjects;

fn storage(e: impl std::fmt::Display) -> ProjectError {
    ProjectError::Storage(e.to_string())
}

fn root() -> PathBuf {
    vault::root().join("projects")
}

fn file_of(name: &str) -> PathBuf {
    root().join(name).join(FILE)
}

/// 임시 파일에 다 쓴 뒤 제자리로 옮긴다. 같은 디렉토리 안이라 rename 이 원자적이다.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
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

impl ProjectStore for FileProjects {
    fn list(&self) -> Vec<Result<ProjectRecord, String>> {
        let Ok(entries) = std::fs::read_dir(root()) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|e| e.path().join(FILE).is_file())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        names.sort();
        names
            .iter()
            .map(|name| {
                self.load(name)
                    .map_err(|e| format!("projects/{name}/{FILE}를 읽지 못했습니다: {e}"))
            })
            .collect()
    }

    fn load(&self, name: &str) -> Result<ProjectRecord, ProjectError> {
        let text = std::fs::read_to_string(file_of(name))
            .map_err(|_| ProjectError::Missing(format!("프로젝트 {name}")))?;
        toml::from_str(&text).map_err(storage)
    }

    fn insert(&self, record: &ProjectRecord) -> Result<(), ProjectError> {
        let path = file_of(&record.name);
        if path.exists() {
            return Err(ProjectError::Taken(record.name.clone()));
        }
        let dir = root().join(&record.name);
        vault::create_private(&root()).map_err(storage)?;
        vault::create_private(&dir).map_err(storage)?;
        let text = toml::to_string_pretty(record).map_err(storage)?;
        write_atomically(&path, text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::tests_support::with_temp_root;
    use secrets_core::project::Origin;

    fn record(name: &str) -> ProjectRecord {
        ProjectRecord {
            name: name.into(),
            group: "개인".into(),
            path: "/w/ledger".into(),
            origin: Origin::Created,
            created_at: "2026-09-24T10:00:00+09:00".into(),
        }
    }

    #[test]
    fn an_inserted_record_reads_back_the_same_and_is_private() {
        with_temp_root(|dir| {
            FileProjects.insert(&record("ledger")).unwrap();
            assert_eq!(FileProjects.load("ledger").unwrap(), record("ledger"));
            assert_eq!(FileProjects.list().len(), 1);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(dir.join("projects/ledger").join(FILE))
                    .unwrap()
                    .permissions()
                    .mode();
                assert_eq!(mode & 0o777, 0o600);
            }
        });
    }

    #[test]
    fn inserting_the_same_name_twice_is_refused_and_keeps_the_first() {
        with_temp_root(|_| {
            FileProjects.insert(&record("ledger")).unwrap();
            let mut second = record("ledger");
            second.group = "tuk".into();
            assert!(matches!(FileProjects.insert(&second), Err(ProjectError::Taken(_))));
            assert_eq!(FileProjects.load("ledger").unwrap().group, "개인");
        });
    }

    #[test]
    fn a_broken_record_is_listed_as_an_error() {
        with_temp_root(|dir| {
            FileProjects.insert(&record("ledger")).unwrap();
            std::fs::create_dir_all(dir.join("projects/broken")).unwrap();
            std::fs::write(dir.join("projects/broken").join(FILE), "name = ").unwrap();
            let listed = FileProjects.list();
            assert_eq!(listed.len(), 2);
            assert_eq!(listed.iter().filter(|r| r.is_err()).count(), 1);
        });
    }

    #[test]
    fn a_folder_without_a_record_is_not_a_project() {
        with_temp_root(|dir| {
            std::fs::create_dir_all(dir.join("projects/half")).unwrap();
            assert!(FileProjects.list().is_empty());
        });
    }
}
