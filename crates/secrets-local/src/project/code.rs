//! 서버에 코드를 받는다 — 배포 계정으로 들어가 레포 키를 두고 `/srv/<레포>` 로 clone 한다.
//!
//! 스크립트 하나를 stdin 으로 넘긴다. 개인 키도 그 안에 들어 있어 명령행이나 로그에 남지 않는다.
//! 배포 경로가 비어 있지 않으면 아무것도 하지 않고 실패한다 — 서버의 파일을 덮어쓰지 않는다.

use secrets_core::key::RepoRef;
use secrets_core::port::ProgressSink;
use secrets_core::project::git_link::ssh_url;
use secrets_core::project::{ProjectError, ServerCode, ServerSeat};

use crate::hosts::{script, ssh};

pub struct SshCode;

/// heredoc 끝 표시. 개인 키 본문에는 나올 수 없는 줄이다.
const KEY_END: &str = "SECRETS_DEPLOY_KEY_END";

/// 키를 두고 clone 하는 스크립트. 키는 그 서버 계정의 `~/.ssh/github/<키 이름>` 에 둔다.
struct Clone<'a> {
    url: &'a str,
    branch: &'a str,
    dest: &'a str,
    key_name: &'a str,
    private_key: &'a str,
}

impl Clone<'_> {
    fn script(&self) -> String {
        format!(
            r#"set -eu
URL={url}
BRANCH={branch}
DEST={dest}
KEYFILE="$HOME/.ssh/github/"{key_name}
SSHCMD="ssh -i $KEYFILE -o IdentitiesOnly=yes"
command -v git >/dev/null 2>&1 || {{ echo "git이 설치되어 있지 않습니다" >&2; exit 1; }}
if [ -e "$DEST" ] && [ -n "$(ls -A "$DEST" 2>/dev/null)" ]; then
  echo "$DEST이(가) 비어 있지 않아 받지 않았습니다" >&2; exit 1
fi
umask 077
mkdir -p "$(dirname "$KEYFILE")"
chmod 700 "$(dirname "$(dirname "$KEYFILE")")" "$(dirname "$KEYFILE")"
cat > "$KEYFILE.tmp" <<'{end}'
{key}
{end}
chmod 600 "$KEYFILE.tmp"
mv "$KEYFILE.tmp" "$KEYFILE"
echo "key-placed $KEYFILE"
umask 002
# github.com 호스트 키는 처음 받을 때만 받아 둔다. 레포 설정에는 키 지정만 남긴다.
GIT_SSH_COMMAND="$SSHCMD -o StrictHostKeyChecking=accept-new" git clone --quiet --branch "$BRANCH" "$URL" "$DEST"
git -C "$DEST" config core.sshCommand "$SSHCMD"
echo "cloned $DEST ($BRANCH)"
"#,
            url = script::quote(self.url),
            branch = script::quote(self.branch),
            dest = script::quote(self.dest),
            key_name = script::quote(self.key_name),
            key = self.private_key.trim_end(),
            end = KEY_END,
        )
    }
}

impl ServerCode for SshCode {
    fn clone_repository(
        &self,
        seat: &ServerSeat,
        repo: &RepoRef,
        branch: &str,
        path: &str,
        private_key: &str,
        progress: &dyn ProgressSink,
    ) -> Result<(), ProjectError> {
        let key = std::fs::read_to_string(private_key)
            .map_err(|e| ProjectError::Storage(format!("배포 키를 읽지 못했습니다: {e}")))?;
        if key.lines().any(|l| l == KEY_END) {
            return Err(ProjectError::Invalid(
                "배포 키 파일의 형식이 올바르지 않습니다.".into(),
            ));
        }
        let text = Clone {
            url: &ssh_url(repo),
            branch,
            dest: path,
            key_name: repo.name(),
            private_key: &key,
        }
        .script();
        let account_key = super::server::private_key(seat)?;
        ssh::run(
            &account_key.display().to_string(),
            &seat.login,
            &seat.address,
            &text,
            progress,
        )
        .map(|_| ())
        .map_err(|e| ProjectError::Storage(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::workspace::tests_support::TempDir;

    /// 스크립트를 이 머신의 셸로 돌린다. ssh 만 빼고 같은 것이다. 홈은 임시 디렉토리다.
    fn run_here(home: &TempDir, text: &str) -> std::process::Output {
        std::process::Command::new("bash")
            .arg("-c")
            .arg(text)
            .env("HOME", home.path())
            .output()
            .unwrap()
    }

    fn git(dir: &std::path::Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// 원격 대신 쓸 로컬 bare 레포. 커밋이 하나 있다.
    fn origin(dir: &TempDir) -> std::path::PathBuf {
        let work = dir.path().join("work");
        std::fs::create_dir(&work).unwrap();
        git(&work, &["init", "-q", "--initial-branch=main"]);
        std::fs::write(work.join("README.md"), "hi").unwrap();
        git(
            &work,
            &["-c", "user.name=t", "-c", "user.email=t@t", "add", "."],
        );
        git(
            &work,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "-m",
                "first",
            ],
        );
        let bare = dir.path().join("origin.git");
        std::process::Command::new("git")
            .args(["clone", "-q", "--bare"])
            .arg(&work)
            .arg(&bare)
            .output()
            .unwrap();
        bare
    }

    fn clone_of(
        bare: &std::path::Path,
        branch: &str,
        dest: &std::path::Path,
        private_key: &str,
    ) -> String {
        Clone {
            url: &bare.display().to_string(),
            branch,
            dest: &dest.display().to_string(),
            key_name: "api",
            private_key,
        }
        .script()
    }

    #[test]
    fn a_branch_that_does_not_exist_fails_without_leaving_a_checkout() {
        let world = TempDir::new("code-branch");
        let home = TempDir::new("code-branch-home");
        let bare = origin(&world);
        let dest = world.path().join("srv-api");

        let out = run_here(&home, &clone_of(&bare, "develop", &dest, "k"));

        assert!(!out.status.success());
        assert!(!dest.join(".git").exists());
    }

    #[test]
    fn places_the_key_privately_clones_and_pins_the_key_to_the_repository() {
        let world = TempDir::new("code-world");
        let home = TempDir::new("code-home");
        let bare = origin(&world);
        let dest = world.path().join("srv-api");
        let keyfile = home.path().join(".ssh/github/api");
        let command = format!("ssh -i {} -o IdentitiesOnly=yes", keyfile.display());

        let out = run_here(
            &home,
            &clone_of(
                &bare,
                "main",
                &dest,
                "-----BEGIN KEY-----\nabc\n-----END KEY-----\n",
            ),
        );
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );

        assert_eq!(
            std::fs::read_to_string(&keyfile).unwrap(),
            "-----BEGIN KEY-----\nabc\n-----END KEY-----\n"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&keyfile).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert!(dest.join("README.md").is_file());
        assert_eq!(git(&dest, &["config", "core.sshCommand"]), command);
        assert_eq!(git(&dest, &["rev-parse", "--abbrev-ref", "HEAD"]), "main");
        assert!(
            !String::from_utf8_lossy(&out.stdout).contains("abc"),
            "키 본문이 출력에 나오면 안 된다"
        );
    }

    #[test]
    fn an_existing_empty_directory_is_filled() {
        let world = TempDir::new("code-empty");
        let home = TempDir::new("code-empty-home");
        let bare = origin(&world);
        let dest = world.path().join("srv-api");
        std::fs::create_dir(&dest).unwrap();

        let out = run_here(&home, &clone_of(&bare, "main", &dest, "k"));
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(dest.join("README.md").is_file());
    }

    #[test]
    fn a_directory_with_files_is_left_untouched_and_no_key_is_placed() {
        let world = TempDir::new("code-full");
        let home = TempDir::new("code-full-home");
        let bare = origin(&world);
        let dest = world.path().join("srv-api");
        std::fs::create_dir(&dest).unwrap();
        std::fs::write(dest.join("keep.txt"), "mine").unwrap();

        let out = run_here(&home, &clone_of(&bare, "main", &dest, "k"));

        assert!(!out.status.success());
        assert_eq!(
            std::fs::read_to_string(dest.join("keep.txt")).unwrap(),
            "mine"
        );
        assert!(!home.path().join(".ssh/github/api").exists());
    }
}
