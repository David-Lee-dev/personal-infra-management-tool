//! 프로젝트 기록. 기록은 덮어쓰지 않고 새로 쓰기만 한다.
//!
//! 이름을 바꾸면 기록 디렉토리째 옮기고, 빼는 것은 지우지 않고 보관소로 옮긴다.

use std::path::{Path, PathBuf};

use secrets_core::project::{ProjectError, ProjectFiles, ProjectRecord, ProjectStore};

use crate::{clock, vault};

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

    fn replace(&self, record: &ProjectRecord) -> Result<(), ProjectError> {
        let path = file_of(&record.name);
        if !path.is_file() {
            return Err(ProjectError::Missing(format!("프로젝트 {}", record.name)));
        }
        let text = toml::to_string_pretty(record).map_err(storage)?;
        write_atomically(&path, text.as_bytes())
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

/// 기록 디렉토리 이름으로 쓸 수 있는가. 이름은 기록에서 검사를 거친 것이지만 한 번 더 막는다.
fn check_part(part: &str) -> Result<(), ProjectError> {
    if part.is_empty() || part.contains('/') || part == "." || part == ".." {
        return Err(ProjectError::Invalid(format!(
            "{part}은(는) 경로 이름으로 쓸 수 없습니다."
        )));
    }
    Ok(())
}

fn archive_root() -> PathBuf {
    vault::root().join("archive").join("projects")
}

/// 보관한 까닭과 시각을 옆에 적는다.
fn note(dir: &Path, reason: &str) {
    let text = format!(
        "archived_at = \"{}\"\nreason = \"{reason}\"\n",
        clock::now()
    );
    let _ = vault::write_private(&dir.join("archived.toml"), text.as_bytes());
}

impl ProjectFiles for FileProjects {
    fn rename_project(&self, from: &str, to: &str) -> Result<(), ProjectError> {
        check_part(from)?;
        check_part(to)?;
        let (source, target) = (root().join(from), root().join(to));
        if target.exists() {
            return Err(ProjectError::Taken(to.to_string()));
        }
        std::fs::rename(&source, &target).map_err(storage)
    }

    fn archive_project(&self, name: &str) -> Result<String, ProjectError> {
        check_part(name)?;
        let parent = archive_root();
        vault::create_private(&parent).map_err(storage)?;
        let kept = parent.join(format!("{}-{name}", clock::stamp()));
        std::fs::rename(root().join(name), &kept).map_err(storage)?;
        note(&kept, "등록 해제");
        Ok(kept.display().to_string())
    }

    fn rename_environment(&self, project: &str, from: &str, to: &str) -> Result<(), ProjectError> {
        check_part(project)?;
        check_part(from)?;
        check_part(to)?;
        let deploy = root().join(project).join("deploy");
        let (source, target) = (deploy.join(from), deploy.join(to));
        if !source.exists() {
            return Ok(());
        }
        if target.exists() {
            return Err(ProjectError::Taken(format!("deploy/{to}")));
        }
        std::fs::rename(&source, &target).map_err(storage)
    }

    fn archive_environment(
        &self,
        project: &str,
        environment: &str,
        record: &str,
    ) -> Result<String, ProjectError> {
        check_part(project)?;
        check_part(environment)?;
        let kept = archive_root()
            .join(project)
            .join("environments")
            .join(format!("{}-{environment}", clock::stamp()));
        vault::create_private(&kept).map_err(storage)?;
        vault::write_private(&kept.join("environment.toml"), record.as_bytes()).map_err(storage)?;
        let scripts = root().join(project).join("deploy").join(environment);
        if scripts.exists() {
            std::fs::rename(&scripts, kept.join("deploy")).map_err(storage)?;
        }
        note(&kept, "환경 빼기");
        Ok(kept.display().to_string())
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
            environments: Vec::new(),
        }
    }

    mod files {
        use super::*;

        fn with_script(root: &Path, project: &str, env: &str) {
            let dir = root.join("projects").join(project).join("deploy").join(env);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("script.sh"), "echo\n").unwrap();
        }

        #[test]
        fn renaming_a_project_moves_its_deploy_scripts_and_refuses_a_taken_name() {
            with_temp_root(|root| {
                FileProjects.insert(&record("ledger")).unwrap();
                FileProjects.insert(&record("other")).unwrap();
                with_script(root, "ledger", "prod");

                assert!(FileProjects.rename_project("ledger", "other").is_err());
                FileProjects.rename_project("ledger", "books").unwrap();

                assert!(root.join("projects/books/deploy/prod/script.sh").is_file());
                assert!(!root.join("projects/ledger").exists());
            });
        }

        #[test]
        fn unregistering_moves_the_whole_record_to_the_archive() {
            with_temp_root(|root| {
                FileProjects.insert(&record("ledger")).unwrap();
                with_script(root, "ledger", "prod");

                let kept = PathBuf::from(FileProjects.archive_project("ledger").unwrap());

                assert!(kept.starts_with(root.join("archive/projects")));
                assert!(kept.join(FILE).is_file());
                assert!(kept.join("deploy/prod/script.sh").is_file());
                assert!(kept.join("archived.toml").is_file());
                assert!(FileProjects.list().is_empty());
            });
        }

        #[test]
        fn an_environment_rename_moves_its_scripts_and_removal_keeps_record_and_scripts() {
            with_temp_root(|root| {
                FileProjects.insert(&record("ledger")).unwrap();
                with_script(root, "ledger", "prod");

                FileProjects
                    .rename_environment("ledger", "prod", "live")
                    .unwrap();
                assert!(root.join("projects/ledger/deploy/live/script.sh").is_file());
                // 스크립트가 없는 환경은 옮길 것이 없다.
                FileProjects
                    .rename_environment("ledger", "dev", "qa")
                    .unwrap();

                let kept = PathBuf::from(
                    FileProjects
                        .archive_environment("ledger", "live", "name = \"live\"\n")
                        .unwrap(),
                );
                assert_eq!(
                    std::fs::read_to_string(kept.join("environment.toml")).unwrap(),
                    "name = \"live\"\n"
                );
                assert!(kept.join("deploy/script.sh").is_file());
                assert!(!root.join("projects/ledger/deploy/live").exists());
            });
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
            assert!(matches!(
                FileProjects.insert(&second),
                Err(ProjectError::Taken(_))
            ));
            assert_eq!(FileProjects.load("ledger").unwrap().group, "개인");
        });
    }

    #[test]
    fn a_record_without_environments_still_reads_and_replace_keeps_them() {
        with_temp_root(|dir| {
            std::fs::create_dir_all(dir.join("projects/old")).unwrap();
            std::fs::write(
                dir.join("projects/old").join(FILE),
                "name = \"old\"\ngroup = \"g\"\npath = \"/w/old\"\norigin = \"registered\"\ncreated_at = \"t\"\n",
            )
            .unwrap();
            let mut old = FileProjects.load("old").unwrap();
            assert!(old.environments.is_empty());

            old.environments.push(secrets_core::project::Environment {
                name: "prod".into(),
                server: "i-1".into(),
                instance: None,
                address: None,
                login: "deploy".into(),
                path: "/srv/old".into(),
                branch: "main".into(),
                connected_at: "t".into(),
                env_file: None,
                server_env_file: ".env".into(),
            });
            FileProjects.replace(&old).unwrap();
            assert_eq!(FileProjects.load("old").unwrap(), old);
        });
    }

    #[test]
    fn an_environment_written_before_servers_keeps_its_instance_and_address_until_linked() {
        with_temp_root(|dir| {
            std::fs::create_dir_all(dir.join("projects/api")).unwrap();
            std::fs::write(
                dir.join("projects/api").join(FILE),
                r#"name = "api"
group = "g"
path = "/w/api"
origin = "registered"
created_at = "t"

[[environments]]
name = "prod"
aws_account = "320042238085"
machine = "ec2"
region = "ap-northeast-2"
keypair = "tuk-key"
instance = "i-0b97"
instance_name = "tuk-api-server"
address = "54.116.119.214"
login = "deploy"
path = "/srv/api"
branch = "main"
connected_at = "t"
server_env_file = ".env"
"#,
            )
            .unwrap();

            let mut api = FileProjects.load("api").unwrap();
            let env = &api.environments[0];
            assert_eq!(env.server, "");
            assert_eq!(env.instance.as_deref(), Some("i-0b97"));
            assert_eq!(env.address.as_deref(), Some("54.116.119.214"));

            api.environments[0].server = "tuk-api-server".into();
            api.environments[0].instance = None;
            api.environments[0].address = None;
            FileProjects.replace(&api).unwrap();
            let text = std::fs::read_to_string(dir.join("projects/api").join(FILE)).unwrap();
            assert!(text.contains("server = \"tuk-api-server\""), "{text}");
            assert!(!text.contains("aws_account") && !text.contains("instance"), "{text}");
        });
    }

    #[test]
    fn replacing_a_record_that_was_never_inserted_is_refused() {
        with_temp_root(|_| {
            assert!(matches!(
                FileProjects.replace(&record("ghost")),
                Err(ProjectError::Missing(_))
            ));
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
