//! 서버 기록 — 서버 한 대가 `servers/<id>.toml` 하나다.
//!
//! 빼는 것은 지우지 않고 보관소로 옮긴다.

use std::path::{Path, PathBuf};

use secrets_core::server::{Server, ServerError, ServerStore};

use crate::{clock, vault};

pub struct FileServers {
    /// 기록이 놓이는 곳 (`~/.secrets/servers`).
    pub dir: PathBuf,
    /// 등록 해제한 기록을 옮기는 곳.
    pub archive: PathBuf,
}

impl FileServers {
    pub fn standard() -> FileServers {
        FileServers {
            dir: vault::root().join("servers"),
            archive: vault::root().join("archive").join("servers"),
        }
    }

    fn file_of(&self, id: &str) -> Result<PathBuf, ServerError> {
        let valid = !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !valid {
            return Err(ServerError::Invalid(format!(
                "{id}은(는) 서버 id로 쓸 수 없습니다."
            )));
        }
        Ok(self.dir.join(format!("{id}.toml")))
    }
}

fn storage(e: impl std::fmt::Display) -> ServerError {
    ServerError::Storage(e.to_string())
}

/// 임시 파일에 다 쓴 뒤 제자리로 옮긴다. 같은 디렉토리 안이라 rename 이 원자적이다.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), ServerError> {
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

impl ServerStore for FileServers {
    fn list(&self) -> Vec<Result<Server, String>> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut ids: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .filter_map(|name| name.strip_suffix(".toml").map(str::to_string))
            .collect();
        ids.sort();
        ids.iter()
            .map(|id| {
                self.load(id)
                    .map_err(|e| format!("servers/{id}.toml을 읽지 못했습니다: {e}"))
            })
            .collect()
    }

    fn load(&self, id: &str) -> Result<Server, ServerError> {
        let text = std::fs::read_to_string(self.file_of(id)?)
            .map_err(|_| ServerError::Missing(format!("서버 {id}")))?;
        toml::from_str(&text).map_err(storage)
    }

    fn insert(&self, server: &Server) -> Result<(), ServerError> {
        let path = self.file_of(&server.id)?;
        if path.exists() {
            return Err(ServerError::Taken(format!("서버 {}", server.id)));
        }
        vault::create_private(&self.dir).map_err(storage)?;
        let text = toml::to_string_pretty(server).map_err(storage)?;
        write_atomically(&path, text.as_bytes())
    }

    fn replace(&self, server: &Server) -> Result<(), ServerError> {
        let path = self.file_of(&server.id)?;
        if !path.is_file() {
            return Err(ServerError::Missing(format!("서버 {}", server.id)));
        }
        let text = toml::to_string_pretty(server).map_err(storage)?;
        write_atomically(&path, text.as_bytes())
    }

    fn archive(&self, id: &str) -> Result<(), ServerError> {
        let path = self.file_of(id)?;
        if !path.is_file() {
            return Err(ServerError::Missing(format!("서버 {id}")));
        }
        vault::create_private(&self.archive).map_err(storage)?;
        std::fs::rename(
            &path,
            self.archive.join(format!("{}-{id}.toml", clock::stamp())),
        )
        .map_err(storage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::tests_support::TempDir;
    use secrets_core::server::ServerKind;

    fn servers(dir: &TempDir) -> FileServers {
        FileServers {
            dir: dir.path().join("vault/servers"),
            archive: dir.path().join("vault/archive/servers"),
        }
    }

    fn nemo() -> Server {
        toml::from_str(
            r#"
id = "nemo"
name = "nemo"
address = "nemo.tail25dc19.ts.net"
kind = "other"
registered_at = "t"

[[accounts]]
login = "infra"
role = "user"
key = { kind = "agent" }
origin = "registered"
state = "unverified"
"#,
        )
        .unwrap()
    }

    #[test]
    fn a_record_round_trips_with_defaults_and_is_private() {
        let dir = TempDir::new("servers");
        let store = servers(&dir);
        let server = nemo();
        assert_eq!(
            (server.port, server.kind, server.workspace.as_str()),
            (22, ServerKind::Other, "/srv")
        );

        store.insert(&server).unwrap();
        assert_eq!(store.load("nemo").unwrap(), server);
        assert!(matches!(store.insert(&server), Err(ServerError::Taken(_))));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(store.dir.join("nemo.toml"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn archiving_moves_the_record_out_of_the_list() {
        let dir = TempDir::new("servers-archive");
        let store = servers(&dir);
        store.insert(&nemo()).unwrap();

        store.archive("nemo").unwrap();

        assert!(store.list().is_empty());
        assert_eq!(std::fs::read_dir(&store.archive).unwrap().count(), 1);
    }

    #[test]
    fn an_id_that_is_not_a_plain_name_never_becomes_a_path() {
        let dir = TempDir::new("servers-id");
        let store = servers(&dir);
        assert!(matches!(store.load("../x"), Err(ServerError::Invalid(_))));
    }
}
