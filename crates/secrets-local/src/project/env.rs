//! 환경 변수 파일 — 로컬 파일과 서버 파일을 같은 스크립트로 해시하고, 서버에 올린다.
//!
//! 해시는 로컬과 서버에서 **같은 bash 스크립트**로 만든다. 줄을 나누고 변수 이름을 떼는 방식이
//! 양쪽에서 조금이라도 다르면 같은 파일도 다르게 보이기 때문이다. 스크립트는 값을 출력하지 않고
//! salt 를 섞은 해시와 변수 이름만 출력한다.
//!
//! 올리는 내용은 base64 로 stdin 스크립트에 싣는다. 명령행이나 작업 로그에는 남지 않는다.

use secrets_core::port::{ProgressSink, Silent};
use secrets_core::project::{
    EnvDigest, LocalEnvFiles, ProjectError, RepoTracking, ServerEnv, ServerEnvFiles, ServerSeat,
};

use crate::hosts::{script, ssh};

/// 배포 경로 `dir` 의 `file` 을 해시로 읽는 스크립트. 바꾸는 명령은 없다.
///
/// 변수 줄: 앞 공백을 뗀 뒤 `#` 로 시작하지 않고 `=` 가 있는 줄. 이름은 첫 `=` 앞에서
/// `export ` 와 양끝 공백을 뗀 것, 값은 첫 `=` 뒤 그대로(따옴표 포함)다. 줄 끝 `\r` 은 뗀다.
fn digest_script(dir: &str, file: &str, salt: &str) -> String {
    format!(
        r#"set -u
d={dir}
f="$d/"{file}
SALT={salt}
h() {{ if command -v sha256sum >/dev/null 2>&1; then sha256sum; else shasum -a 256; fi | cut -c1-64; }}
tracking() {{
  if [ ! -e "$d/.git" ]; then echo "tracking=none"; return; fi
  g() {{ git -c safe.directory='*' -C "$d" "$@" >/dev/null 2>&1; }}
  if g ls-files --error-unmatch -- {file}; then echo "tracking=tracked"
  elif g check-ignore -q -- {file}; then echo "tracking=ignored"
  else echo "tracking=unignored"; fi
}}
if [ ! -d "$d" ]; then echo "state=nodir"; exit 0; fi
if [ ! -e "$f" ]; then echo "state=missing"; tracking; exit 0; fi
if [ ! -f "$f" ] || [ ! -r "$f" ]; then echo "state=unreadable"; exit 0; fi
echo "state=present"
echo "file=$( {{ printf '%s\n' "$SALT"; cat "$f"; }} | h)"
while IFS= read -r line || [ -n "$line" ]; do
  line=${{line%$'\r'}}
  t=${{line#"${{line%%[![:space:]]*}}"}}
  case $t in ''|'#'*) continue ;; *=*) ;; *) continue ;; esac
  k=${{t%%=*}}
  v=${{t#*=}}
  case $k in 'export '*) k=${{k#export }} ;; esac
  k=${{k#"${{k%%[![:space:]]*}}"}}
  k=${{k%"${{k##*[![:space:]]}}"}}
  [ -n "$k" ] || continue
  echo "key=$( {{ printf '%s\n' "$SALT"; printf '%s' "$v"; }} | h) $k"
done < "$f"
echo "mode=$(stat -c %a "$f" 2>/dev/null || stat -f %Lp "$f")"
echo "owner=$(stat -c %U "$f" 2>/dev/null || stat -f %Su "$f")"
tracking
"#,
        dir = script::quote(dir),
        file = script::quote(file),
        salt = script::quote(salt),
    )
}

fn parse_digest(text: &str) -> Result<ServerEnv, ProjectError> {
    let value = |key: &str| {
        text.lines()
            .find_map(|l| {
                l.trim_end()
                    .strip_prefix(&format!("{key}="))
                    .map(str::to_string)
            })
            .filter(|v| !v.is_empty())
    };
    let tracking = || match value("tracking").as_deref() {
        Some("tracked") => RepoTracking::Tracked,
        Some("ignored") => RepoTracking::Ignored,
        Some("unignored") => RepoTracking::Unignored,
        _ => RepoTracking::NoRepository,
    };
    match value("state").as_deref() {
        Some("nodir") => Ok(ServerEnv::NoDirectory),
        Some("missing") => Ok(ServerEnv::Missing {
            tracking: tracking(),
        }),
        Some("unreadable") => Err(ProjectError::Invalid(
            "이 계정으로는 서버의 환경 변수 파일을 읽을 수 없습니다(권한 없음 또는 파일이 아님)."
                .into(),
        )),
        Some("present") => {
            let file = value("file")
                .ok_or_else(|| ProjectError::Storage("파일 해시를 읽지 못했습니다.".into()))?;
            let keys = text
                .lines()
                .filter_map(|l| l.strip_prefix("key="))
                .filter_map(|rest| rest.split_once(' '))
                .map(|(hash, name)| (name.to_string(), hash.to_string()))
                .collect();
            Ok(ServerEnv::Present {
                digest: EnvDigest { file, keys },
                mode: value("mode"),
                owner: value("owner"),
                tracking: tracking(),
            })
        }
        _ => Err(ProjectError::Storage(
            "환경 변수 파일을 읽은 응답을 해석하지 못했습니다.".into(),
        )),
    }
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// 76 자마다 줄을 바꾼 base64.
fn base64(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(BASE64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out.as_bytes()
        .chunks(76)
        .map(|line| String::from_utf8_lossy(line).into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

/// `<dir>/<file>` 을 내용으로 바꾸는 스크립트. 같은 디렉토리의 임시 파일에 쓰고 옮긴다.
fn write_script(dir: &str, file: &str, contents: &[u8]) -> String {
    format!(
        r#"set -eu
d={dir}
f="$d/"{file}
[ -d "$d" ] || {{ echo "배포 경로 $d이(가) 없습니다. 코드를 먼저 받으세요" >&2; exit 1; }}
command -v base64 >/dev/null 2>&1 || {{ echo "서버에 base64가 없습니다" >&2; exit 1; }}
umask 077
tmp="$d/.secrets-upload.$$"
trap 'rm -f "$tmp"' EXIT
base64 -d > "$tmp" <<'SECRETS_ENV_END'
{body}
SECRETS_ENV_END
chmod 600 "$tmp"
mv -f "$tmp" "$f"
trap - EXIT
echo "written $f"
"#,
        dir = script::quote(dir),
        file = script::quote(file),
        body = base64(contents),
    )
}

fn random_hex() -> String {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut bytes);
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 로컬 프로젝트 뿌리의 파일.
pub struct LocalEnv;

impl LocalEnvFiles for LocalEnv {
    fn salt(&self) -> String {
        random_hex()
    }

    fn digest(
        &self,
        project_path: &str,
        file: &str,
        salt: &str,
    ) -> Result<EnvDigest, ProjectError> {
        check_name(file)?;
        let out = std::process::Command::new("bash")
            .arg("-c")
            .arg(digest_script(project_path, file, salt))
            .output()
            .map_err(|e| ProjectError::Storage(format!("bash를 실행하지 못했습니다: {e}")))?;
        match parse_digest(&String::from_utf8_lossy(&out.stdout))? {
            ServerEnv::Present { digest, .. } => Ok(digest),
            ServerEnv::NoDirectory | ServerEnv::Missing { .. } => {
                Err(ProjectError::Missing(format!("{project_path}/{file}")))
            }
        }
    }

    fn read(&self, project_path: &str, file: &str) -> Result<Vec<u8>, ProjectError> {
        check_name(file)?;
        let path = std::path::Path::new(project_path).join(file);
        std::fs::read(&path).map_err(|e| {
            ProjectError::Storage(format!("{}을(를) 읽지 못했습니다: {e}", path.display()))
        })
    }
}

/// 뿌리의 파일 이름만 받는다.
fn check_name(file: &str) -> Result<(), ProjectError> {
    if file.is_empty() || file.contains('/') || file == "." || file == ".." {
        return Err(ProjectError::Invalid(format!(
            "{file}은(는) 프로젝트 뿌리의 파일 이름이 아닙니다."
        )));
    }
    Ok(())
}

/// 서버 계정으로 들어가 배포 경로 뿌리의 환경 변수 파일을 읽고 쓴다.
pub struct SshEnv;

impl ServerEnvFiles for SshEnv {
    fn digest(
        &self,
        seat: &ServerSeat,
        dir: &str,
        file: &str,
        salt: &str,
        progress: &dyn ProgressSink,
    ) -> Result<ServerEnv, ProjectError> {
        check_name(file)?;
        let key = super::server::private_key(seat)?;
        progress.line(
            secrets_core::port::Channel::Out,
            &format!("{dir}/{file} 해시 비교 — 값은 옮기지 않습니다"),
        );
        // 해시 줄은 작업 로그에 싣지 않는다.
        let text = ssh::run(
            &key.display().to_string(),
            &seat.login,
            &seat.address,
            &digest_script(dir, file, salt),
            &Silent,
        )
        .map_err(|e| ProjectError::Storage(e.to_string()))?;
        parse_digest(&text)
    }

    fn write(
        &self,
        seat: &ServerSeat,
        dir: &str,
        file: &str,
        contents: &[u8],
        progress: &dyn ProgressSink,
    ) -> Result<(), ProjectError> {
        check_name(file)?;
        let key = super::server::private_key(seat)?;
        let text = ssh::run(
            &key.display().to_string(),
            &seat.login,
            &seat.address,
            &write_script(dir, file, contents),
            progress,
        )
        .map_err(|e| ProjectError::Storage(e.to_string()))?;
        if !text.lines().any(|l| l.starts_with("written ")) {
            return Err(ProjectError::Storage(format!(
                "서버에 {file}을(를) 쓰지 못했습니다."
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::workspace::tests_support::TempDir;

    fn run_here(text: &str) -> String {
        let out = std::process::Command::new("bash")
            .arg("-c")
            .arg(text)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn digest_here(dir: &TempDir, file: &str, salt: &str) -> ServerEnv {
        parse_digest(&run_here(&digest_script(
            &dir.path().display().to_string(),
            file,
            salt,
        )))
        .unwrap()
    }

    fn present(env: ServerEnv) -> EnvDigest {
        match env {
            ServerEnv::Present { digest, .. } => digest,
            other => panic!("파일이 있어야 한다: {other:?}"),
        }
    }

    mod digest_script {
        use super::*;

        #[test]
        fn names_variables_without_printing_values() {
            let dir = TempDir::new("env-digest");
            dir.write(
                ".env.prod",
                "# c\n\nexport  A = x\nB=\"secret value\"\r\n  C=1=2\nnot a var\nD=last",
            );
            let text = run_here(&digest_script(
                &dir.path().display().to_string(),
                ".env.prod",
                "s",
            ));
            assert!(
                !text.contains("secret value") && !text.contains("=x"),
                "{text}"
            );

            let digest = present(parse_digest(&text).unwrap());
            assert_eq!(
                digest.keys.keys().collect::<Vec<_>>(),
                vec!["A", "B", "C", "D"]
            );
        }

        #[test]
        fn the_same_content_hashes_the_same_and_the_salt_changes_everything() {
            let a = TempDir::new("env-a");
            let b = TempDir::new("env-b");
            a.write(".env.prod", "A=1\nB=2\n");
            b.write(".env", "A=1\nB=2\n");
            assert_eq!(
                present(digest_here(&a, ".env.prod", "s")),
                present(digest_here(&b, ".env", "s"))
            );

            let other = present(digest_here(&a, ".env.prod", "t"));
            assert_ne!(other, present(digest_here(&a, ".env.prod", "s")));
        }

        #[test]
        fn a_comment_changes_the_file_but_not_the_variables() {
            let a = TempDir::new("env-c1");
            let b = TempDir::new("env-c2");
            a.write(".env", "A=1\n");
            b.write(".env", "# note\nA=1\n");
            let (x, y) = (
                present(digest_here(&a, ".env", "s")),
                present(digest_here(&b, ".env", "s")),
            );
            assert_ne!(x.file, y.file);
            assert_eq!(x.keys, y.keys);
        }

        #[test]
        fn tells_a_missing_directory_from_a_missing_file() {
            let dir = TempDir::new("env-missing");
            assert!(matches!(
                digest_here(&dir, ".env", "s"),
                ServerEnv::Missing {
                    tracking: RepoTracking::NoRepository
                }
            ));
            let text = run_here(&digest_script(
                &dir.path().join("nope").display().to_string(),
                ".env",
                "s",
            ));
            assert_eq!(parse_digest(&text).unwrap(), ServerEnv::NoDirectory);
        }

        #[test]
        fn reports_how_the_repository_treats_the_file() {
            let dir = TempDir::new("env-git");
            let git = |args: &[&str]| {
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(dir.path())
                    .args(args)
                    .output()
                    .unwrap()
            };
            git(&["init", "-q"]);
            dir.write(".env", "A=1\n");
            let tracking = |env| match env {
                ServerEnv::Present { tracking, .. } => tracking,
                other => panic!("{other:?}"),
            };
            assert_eq!(
                tracking(digest_here(&dir, ".env", "s")),
                RepoTracking::Unignored
            );
            dir.write(".gitignore", ".env\n");
            assert_eq!(
                tracking(digest_here(&dir, ".env", "s")),
                RepoTracking::Ignored
            );
        }
    }

    mod write_script {
        use super::*;

        #[test]
        fn replaces_the_file_privately_with_exact_bytes() {
            let dir = TempDir::new("env-write");
            dir.write(".env", "OLD=1\n");
            let contents = "A='it''s'\nB=\"x\ny\"\nSECRETS_ENV_END\n\u{d55c}=1\n".repeat(20);

            run_here(&write_script(
                &dir.path().display().to_string(),
                ".env",
                contents.as_bytes(),
            ));

            assert_eq!(
                std::fs::read_to_string(dir.path().join(".env")).unwrap(),
                contents
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(dir.path().join(".env"))
                    .unwrap()
                    .permissions()
                    .mode();
                assert_eq!(mode & 0o777, 0o600);
            }
            let left: Vec<_> = std::fs::read_dir(dir.path())
                .unwrap()
                .filter_map(Result::ok)
                .map(|e| e.file_name())
                .collect();
            assert_eq!(left.len(), 1, "임시 파일이 남으면 안 된다: {left:?}");
        }

        #[test]
        fn refuses_a_missing_directory() {
            let dir = TempDir::new("env-write-missing");
            let out = std::process::Command::new("bash")
                .arg("-c")
                .arg(write_script(
                    &dir.path().join("nope").display().to_string(),
                    ".env",
                    b"A=1\n",
                ))
                .output()
                .unwrap();
            assert!(!out.status.success());
            assert!(!dir.path().join("nope").exists());
        }
    }

    mod base64 {
        use super::*;

        #[test]
        fn matches_the_system_encoder() {
            for input in ["", "a", "ab", "abc", "abcd", "한글 값=1\n"] {
                let mut child = std::process::Command::new("base64")
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .spawn()
                    .unwrap();
                use std::io::Write;
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(input.as_bytes())
                    .unwrap();
                let out = child.wait_with_output().unwrap();
                assert_eq!(
                    base64(input.as_bytes()),
                    String::from_utf8_lossy(&out.stdout).trim(),
                    "{input}"
                );
            }
        }
    }
}
