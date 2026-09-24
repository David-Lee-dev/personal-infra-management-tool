//! 사용자의 작업 공간 — 프로젝트 디렉토리가 실제로 놓이는 곳.
//!
//! 디렉토리를 만들고 `git init` 하는 것 말고는 쓰지 않는다. 스캔은 이름 · 개수 · 근거
//! 파일만 읽는다. 환경 변수 파일은 변수 줄을 세려고 열지만 값은 어디에도 남기지 않는다.

use std::path::{Path, PathBuf};

use secrets_core::project::{EnvFileFact, LocalScan, PathState, ProjectError, Workspace};

use super::detect;
use super::git::Git;

pub struct LocalWorkspace;

/// 사람이 적은 경로를 절대 경로로. `~` 는 홈으로 풀고, 상대 경로는 받지 않는다.
pub fn absolute(text: &str) -> Result<String, ProjectError> {
    let trimmed = text.trim();
    let expanded = match trimmed.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => {
            let home = std::env::var_os("HOME")
                .ok_or_else(|| ProjectError::Invalid("홈 디렉토리를 알 수 없습니다.".into()))?;
            format!("{}{rest}", PathBuf::from(home).display())
        }
        _ => trimmed.to_string(),
    };
    if !Path::new(&expanded).is_absolute() {
        return Err(ProjectError::Invalid(format!(
            "{trimmed}은(는) 절대 경로가 아닙니다. / 또는 ~로 시작하는 경로를 입력하세요."
        )));
    }
    Ok(expanded.trim_end_matches('/').to_string())
}

/// 폴더를 고를 때 시작하고, 벗어날 수 없는 곳 — `~/workspace`.
pub fn workspace_root() -> Result<PathBuf, ProjectError> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| ProjectError::Invalid("홈 디렉토리를 알 수 없습니다.".into()))?;
    Ok(PathBuf::from(home).join("workspace"))
}

/// `path` 가 `root` 안에 있는가. 심볼릭 링크를 풀어 비교하므로 링크로 빠져나갈 수 없다.
pub fn inside(root: &Path, path: &Path) -> bool {
    match (root.canonicalize(), path.canonicalize()) {
        (Ok(root), Ok(path)) => path.starts_with(root),
        _ => false,
    }
}

/// 빈 디렉토리로 볼 때 무시하는 파일. Finder 가 폴더를 열기만 해도 생긴다.
const NOISE: &[&str] = &[".DS_Store"];

fn git_missing() -> ProjectError {
    ProjectError::Storage("git을 찾을 수 없습니다.".into())
}

impl Workspace for LocalWorkspace {
    fn join(&self, parent: &str, directory: &str) -> String {
        Path::new(parent).join(directory).display().to_string()
    }

    fn state(&self, path: &str) -> PathState {
        let path = Path::new(path);
        let Ok(meta) = std::fs::metadata(path) else {
            return PathState::Missing;
        };
        if !meta.is_dir() {
            return PathState::NotDirectory;
        }
        let occupied = std::fs::read_dir(path)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .any(|e| !NOISE.contains(&e.file_name().to_string_lossy().as_ref()))
            })
            // 읽을 수 없는 디렉토리는 비어 있다고 가정하지 않는다.
            .unwrap_or(true);
        if occupied {
            PathState::OccupiedDirectory
        } else {
            PathState::EmptyDirectory
        }
    }

    fn create_directory(&self, path: &str) -> Result<(), ProjectError> {
        match std::fs::create_dir(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                match self.state(path) {
                    PathState::EmptyDirectory => Ok(()),
                    _ => Err(ProjectError::Invalid(format!("{path}에 이미 파일이 있습니다."))),
                }
            }
            Err(e) => Err(ProjectError::Storage(format!("{path}을(를) 만들지 못했습니다: {e}"))),
        }
    }

    fn init_git(&self, path: &str) -> Result<(), ProjectError> {
        let git = Git::find().ok_or_else(git_missing)?;
        git.init(Path::new(path)).map_err(ProjectError::Storage)
    }

    fn scan(&self, path: &str) -> Result<LocalScan, ProjectError> {
        let dir = Path::new(path);
        if !dir.is_dir() {
            return Err(ProjectError::Missing(path.to_string()));
        }
        let git = Git::find();
        let state = git
            .as_ref()
            .map(|g| g.state(dir))
            .unwrap_or(secrets_core::project::GitState::Absent);
        let in_repository = !matches!(state, secrets_core::project::GitState::Absent);

        let env_files = env_file_names(dir)
            .into_iter()
            .map(|name| {
                let (tracked, ignored) = match (&git, in_repository) {
                    (Some(g), true) => (g.tracks(dir, &name), g.ignores(dir, &name)),
                    _ => (false, None),
                };
                EnvFileFact {
                    variables: count_variables(&dir.join(&name)),
                    name,
                    tracked,
                    ignored,
                }
            })
            .collect();

        let ssh_key = match (&git, in_repository) {
            (Some(g), true) => g.ssh_key(dir),
            _ => None,
        };
        Ok(LocalScan {
            git: state,
            runtimes: detect::runtimes(dir),
            env_files,
            ssh_key,
        })
    }

    fn add_to_gitignore(&self, path: &str, names: &[String]) -> Result<(), ProjectError> {
        let file = Path::new(path).join(".gitignore");
        let existing = std::fs::read_to_string(&file).unwrap_or_default();
        let present: Vec<&str> = existing.lines().map(str::trim).collect();
        let missing: Vec<&String> = names
            .iter()
            .filter(|n| !present.contains(&n.as_str()) && !present.contains(&format!("/{n}").as_str()))
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        let mut text = existing.clone();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        for name in missing {
            text.push_str(name);
            text.push('\n');
        }
        std::fs::write(&file, text)
            .map_err(|e| ProjectError::Storage(format!(".gitignore를 쓰지 못했습니다: {e}")))
    }
}

/// 뿌리의 `.env` 와 `.env.*` 파일. 이름 순.
fn env_file_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name == ".env" || name.starts_with(".env."))
        .collect();
    names.sort();
    names
}

/// `KEY=` 로 시작하는 줄의 개수. 값은 보지 않는다.
fn count_variables(file: &Path) -> usize {
    let Ok(text) = std::fs::read_to_string(file) else {
        return 0;
    };
    text.lines().filter(|line| is_variable_line(line)).count()
}

fn is_variable_line(line: &str) -> bool {
    let line = line.trim_start();
    let line = line.strip_prefix("export ").unwrap_or(line);
    let Some((key, _)) = line.split_once('=') else {
        return false;
    };
    let key = key.trim_end();
    let mut chars = key.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
pub mod tests_support {
    use std::path::{Path, PathBuf};

    /// 테스트마다 따로 쓰는 임시 디렉토리. Drop 될 때 지운다.
    pub struct TempDir(PathBuf);

    impl TempDir {
        pub fn new(label: &str) -> TempDir {
            static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "secrets-project-{}-{label}-{n}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }

        pub fn text(&self) -> String {
            self.0.display().to_string()
        }

        pub fn write(&self, name: &str, body: &str) {
            let file = self.0.join(name);
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(file, body).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::TempDir;
    use super::*;
    use secrets_core::project::GitState;

    mod absolute {
        use super::*;

        #[test]
        fn expands_the_home_tilde() {
            let home = std::env::var("HOME").unwrap();
            assert_eq!(absolute("~/work/").unwrap(), format!("{home}/work"));
        }

        #[test]
        fn refuses_a_relative_path() {
            assert!(absolute("work/x").is_err());
            assert!(absolute("~other/x").is_err());
        }
    }

    mod inside {
        use super::*;

        #[test]
        fn a_folder_under_the_root_is_inside_and_the_root_itself_too() {
            let dir = TempDir::new("inside");
            dir.write("a/b/readme.md", "x");
            assert!(inside(dir.path(), &dir.path().join("a/b")));
            assert!(inside(dir.path(), dir.path()));
        }

        #[test]
        fn a_sibling_and_a_link_that_leads_out_are_outside() {
            let root = TempDir::new("root");
            let other = TempDir::new("other");
            assert!(!inside(root.path(), other.path()));
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(other.path(), root.path().join("escape")).unwrap();
                assert!(!inside(root.path(), &root.path().join("escape")));
            }
        }

        #[test]
        fn a_missing_path_is_not_inside() {
            let dir = TempDir::new("gone");
            assert!(!inside(dir.path(), &dir.path().join("nope")));
        }
    }

    mod state {
        use super::*;

        #[test]
        fn tells_missing_empty_occupied_and_files_apart() {
            let dir = TempDir::new("state");
            std::fs::create_dir(dir.path().join("empty")).unwrap();
            dir.write("finder/.DS_Store", "");
            dir.write("full/a.txt", "x");
            dir.write("file.txt", "x");

            let at = |name: &str| LocalWorkspace.state(&dir.path().join(name).display().to_string());
            assert_eq!(at("nope"), PathState::Missing);
            assert_eq!(at("empty"), PathState::EmptyDirectory);
            assert_eq!(at("finder"), PathState::EmptyDirectory);
            assert_eq!(at("full"), PathState::OccupiedDirectory);
            assert_eq!(at("file.txt"), PathState::NotDirectory);
        }
    }

    mod create_directory {
        use super::*;

        #[test]
        fn reuses_an_empty_directory_but_not_an_occupied_one() {
            let dir = TempDir::new("create");
            let fresh = dir.path().join("fresh").display().to_string();
            LocalWorkspace.create_directory(&fresh).unwrap();
            LocalWorkspace.create_directory(&fresh).unwrap();

            dir.write("full/a.txt", "x");
            let full = dir.path().join("full").display().to_string();
            assert!(LocalWorkspace.create_directory(&full).is_err());
        }
    }

    mod scan {
        use super::*;

        #[test]
        fn a_plain_directory_has_no_git_and_counts_variables_only() {
            let dir = TempDir::new("plain");
            dir.write(".env.local", "# comment\nexport API_KEY=abc\nPORT = 3000\n\nnot a line\n");
            dir.write(".env.example", "API_KEY=\n");
            dir.write(".envrc", "use nix");

            let scan = LocalWorkspace.scan(&dir.text()).unwrap();

            assert_eq!(scan.git, GitState::Absent);
            let names: Vec<(&str, usize)> =
                scan.env_files.iter().map(|f| (f.name.as_str(), f.variables)).collect();
            assert_eq!(names, vec![(".env.example", 1), (".env.local", 2)]);
            assert_eq!(scan.env_files[1].ignored, None);
        }

        #[test]
        fn after_git_init_the_repository_and_ignore_rules_are_seen() {
            if Git::find().is_none() {
                return;
            }
            let dir = TempDir::new("git");
            LocalWorkspace.init_git(&dir.text()).unwrap();
            dir.write(".gitignore", ".env.local\n");
            dir.write(".env.local", "A=1\n");
            dir.write(".env.prod", "A=1\n");

            let scan = LocalWorkspace.scan(&dir.text()).unwrap();

            assert!(matches!(
                scan.git,
                GitState::Local { ref branch, commits: 0, .. } if branch.as_deref() == Some("main")
            ));
            let local = scan.env_files.iter().find(|f| f.name == ".env.local").unwrap();
            let prod = scan.env_files.iter().find(|f| f.name == ".env.prod").unwrap();
            assert_eq!(local.ignored, Some(true));
            assert_eq!(prod.ignored, Some(false));
            assert!(!prod.tracked);
        }

        #[test]
        fn a_subdirectory_of_another_repository_is_not_a_repository() {
            if Git::find().is_none() {
                return;
            }
            let dir = TempDir::new("nested");
            LocalWorkspace.init_git(&dir.text()).unwrap();
            dir.write("inner/readme.md", "x");

            let inner = dir.path().join("inner").display().to_string();
            assert_eq!(LocalWorkspace.scan(&inner).unwrap().git, GitState::Absent);
        }

        #[test]
        fn a_missing_directory_is_an_error() {
            let dir = TempDir::new("missing");
            let gone = dir.path().join("gone").display().to_string();
            assert!(matches!(LocalWorkspace.scan(&gone), Err(ProjectError::Missing(_))));
        }
    }

    mod add_to_gitignore {
        use super::*;

        #[test]
        fn appends_missing_names_and_keeps_what_was_there() {
            let dir = TempDir::new("ignore");
            dir.write(".gitignore", "node_modules\n/.env.dev");
            let names = vec![".env.local".to_string(), ".env.dev".to_string()];

            LocalWorkspace.add_to_gitignore(&dir.text(), &names).unwrap();
            LocalWorkspace.add_to_gitignore(&dir.text(), &names).unwrap();

            let text = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
            assert_eq!(text, "node_modules\n/.env.dev\n.env.local\n");
        }

        #[test]
        fn creates_the_file_when_there_is_none() {
            let dir = TempDir::new("ignore-new");
            LocalWorkspace.add_to_gitignore(&dir.text(), &[".env".to_string()]).unwrap();
            assert_eq!(std::fs::read_to_string(dir.path().join(".gitignore")).unwrap(), ".env\n");
        }
    }

    mod is_variable_line {
        use super::*;

        #[test]
        fn accepts_keys_and_rejects_comments_and_prose() {
            assert!(is_variable_line("KEY=v"));
            assert!(is_variable_line("  export _K2 = v"));
            assert!(!is_variable_line("# KEY=v"));
            assert!(!is_variable_line("1KEY=v"));
            assert!(!is_variable_line("no equals"));
            assert!(!is_variable_line("A-B=v"));
        }
    }
}
