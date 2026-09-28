//! 서버에서 돌 셸 조각.
//!
//! 한 번에 하나씩 원격 명령을 보내면 왕복이 늘고, 중간에 끊기면 반쯤 된 상태가
//! 남는다. 그래서 **한 판을 하나의 스크립트로** 만들어 stdin 으로 넘긴다.
//!
//! 다시 돌려도 같은 결과여야 한다. 실패하면 거기서 멈춰야 하므로 `set -eu` 를 켠다.

/// 셸이 그대로 읽도록 홑따옴표로 감싼다.
///
/// 계정 이름·경로·공개 키가 전부 여기를 지난다. 감싸지 않으면 값 하나로 서버에서
/// 임의 명령이 돌 수 있다.
pub fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// 이 서버가 계정을 받을 준비가 되었는지 본다. 아무것도 바꾸지 않는다.
pub fn inspect() -> String {
    r#"set -u
sudo -n true 2>/dev/null && echo "has:sudo"
command -v useradd >/dev/null 2>&1 && echo "has:useradd"
command -v visudo  >/dev/null 2>&1 && echo "has:visudo"
command -v setfacl >/dev/null 2>&1 && echo "has:setfacl"
for p in apt-get dnf yum apk; do
  command -v "$p" >/dev/null 2>&1 && { echo "packager:$p"; break; }
done
echo ok
"#
    .to_string()
}

/// 모자란 것을 채운다. 지금은 `acl` 하나다.
///
/// 패키지를 까는 건 이 도구가 서버 구성에 손대는 유일한 자리다. 그래서 **사용자가
/// 누를 때만** 돌고, 무엇을 까는지 한 줄로 정해져 있다.
pub fn prepare() -> String {
    r#"set -eu
if ! command -v setfacl >/dev/null 2>&1; then
  if command -v apt-get >/dev/null 2>&1; then
    sudo DEBIAN_FRONTEND=noninteractive apt-get install -y acl       || { sudo apt-get update -qq && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y acl; }
  elif command -v dnf >/dev/null 2>&1; then
    sudo dnf install -y acl
  elif command -v yum >/dev/null 2>&1; then
    sudo yum install -y acl
  elif command -v apk >/dev/null 2>&1; then
    sudo apk add --no-cache acl
  else
    echo "no-packager" >&2
    exit 1
  fi
fi
echo ok
"#
    .to_string()
}

/// 계정·공용 자리·키·권한을 심는다.
pub fn install(
    account: &str,
    role: &str,
    workspace: &str,
    group: &str,
    public_key: &str,
    mode: secrets_core::server::InstallMode,
) -> String {
    let mode = match mode {
        secrets_core::server::InstallMode::New => "new",
        secrets_core::server::InstallMode::Reinstall => "reinstall",
    };
    format!(
        r#"set -eu
ACCOUNT={account}
ROLE={role}
WORKSPACE={workspace}
GROUP={group}
KEY={key}
MODE={mode}
MARK="secrets/$ACCOUNT"
HOME_DIR=$(getent passwd "$ACCOUNT" | cut -d: -f6 || true)

# 새 계정의 이름이 이미 있으면 키 · 권한 · 공용 자리를 바꾸기 전에 멈춘다.
if id -u "$ACCOUNT" >/dev/null 2>&1; then
  if [ "$MODE" = new ]; then echo "account-taken" >&2; exit 1; fi
  echo "account-existing"
elif getent group "$ACCOUNT" >/dev/null 2>&1; then
  # 같은 이름의 그룹이 이미 있다(Ubuntu 의 admin 등). 그 그룹을 기본 그룹으로 쓴다.
  # 다만 그 그룹이 sudo 를 받는다면 user 역할이 몰래 sudo 를 얻으므로 막는다.
  if [ "$ROLE" != admin ] && sudo grep -qsE "^[[:space:]]*%$ACCOUNT[[:space:]]" /etc/sudoers /etc/sudoers.d/*; then
    echo "group-grants-sudo" >&2
    exit 1
  fi
  sudo useradd -m -s /bin/bash -g "$ACCOUNT" "$ACCOUNT"
  echo "account-created"
  HOME_DIR=$(getent passwd "$ACCOUNT" | cut -d: -f6)
else
  sudo useradd -m -s /bin/bash -U "$ACCOUNT"
  echo "account-created"
  HOME_DIR=$(getent passwd "$ACCOUNT" | cut -d: -f6)
fi
[ -n "$HOME_DIR" ] || {{ echo "no-home" >&2; exit 1; }}
# 다른 사용자가 남의 홈을 들여다보지 못하게 한다.
sudo chmod 750 "$HOME_DIR"

# 공용 자리. 모든 계정이 함께 쓴다.
sudo groupadd -f "$GROUP"
sudo usermod -aG "$GROUP" "$ACCOUNT"
sudo install -d -o root -g "$GROUP" -m 2775 "$WORKSPACE"
if command -v setfacl >/dev/null 2>&1; then
  # setgid 는 새 파일의 그룹만 맞춘다. 쓰기 비트는 umask 가 정하므로 기본값이면
  # 상대가 읽기만 된다. default ACL 이 그 구멍을 메운다.
  sudo setfacl -R -m "g:$GROUP:rwX" "$WORKSPACE"
  sudo setfacl -R -d -m "g:$GROUP:rwX" "$WORKSPACE"
else
  echo "no-setfacl" >&2
fi

# 키. 우리 표시가 붙은 줄만 갈아 끼운다 — 남이 넣은 키는 건드리지 않는다.
sudo install -d -o "$ACCOUNT" -g "$ACCOUNT" -m 700 "$HOME_DIR/.ssh"
sudo touch "$HOME_DIR/.ssh/authorized_keys"
TMP=$(mktemp)
sudo cat "$HOME_DIR/.ssh/authorized_keys" | grep -v -- " $MARK$" > "$TMP" || true
printf '%s %s\n' "$KEY" "$MARK" >> "$TMP"
sudo install -o "$ACCOUNT" -g "$ACCOUNT" -m 600 "$TMP" "$HOME_DIR/.ssh/authorized_keys"
rm -f "$TMP"

# 권한. 사용자 역할은 규칙 자체를 만들지 않는다.
RULE="/etc/sudoers.d/secrets-$ACCOUNT"
if [ "$ROLE" = admin ]; then
  T=$(mktemp)
  printf '%s ALL=(ALL) NOPASSWD:ALL\n' "$ACCOUNT" > "$T"
  # 문법이 깨진 파일이 들어가면 그 서버의 sudo 가 통째로 멈춘다.
  sudo visudo -c -f "$T" >/dev/null
  sudo install -o root -g root -m 440 "$T" "$RULE"
  rm -f "$T"
else
  sudo rm -f "$RULE"
fi
echo "ok"
"#,
        account = quote(account),
        role = quote(role),
        workspace = quote(workspace),
        group = quote(group),
        key = quote(public_key),
        mode = quote(mode),
    )
}

/// 권한·키·계정을 걷어낸다. `ours` 가 거짓이면 계정은 남긴다.
pub fn remove(account: &str, group: &str, ours: bool) -> String {
    format!(
        r#"set -eu
ACCOUNT={account}
GROUP={group}
MARK="secrets/$ACCOUNT"
HOME_DIR=$(getent passwd "$ACCOUNT" | cut -d: -f6 || true)

sudo rm -f "/etc/sudoers.d/secrets-$ACCOUNT"

if [ -n "$HOME_DIR" ] && [ -f "$HOME_DIR/.ssh/authorized_keys" ]; then
  TMP=$(mktemp)
  sudo cat "$HOME_DIR/.ssh/authorized_keys" | grep -v -- " $MARK$" > "$TMP" || true
  sudo install -o "$ACCOUNT" -g "$ACCOUNT" -m 600 "$TMP" "$HOME_DIR/.ssh/authorized_keys"
  rm -f "$TMP"
fi

if [ "{ours}" = "yes" ] && id -u "$ACCOUNT" >/dev/null 2>&1; then
  # userdel 은 계정과 이름이 같은 그룹이 비면 같이 지운다. 우리가 만든 그룹(gid 1000
  # 이상)이 아니라 원래 있던 시스템 그룹이면 같은 gid 로 되살린다.
  GID=$(getent group "$ACCOUNT" | cut -d: -f3 || true)
  sudo userdel -r "$ACCOUNT" 2>/dev/null || sudo userdel "$ACCOUNT"
  if [ -n "$GID" ] && [ "$GID" -lt 1000 ] && ! getent group "$ACCOUNT" >/dev/null 2>&1; then
    sudo groupadd -g "$GID" "$ACCOUNT"
  fi
  echo "account-removed"
else
  # 우리가 만들지 않은 계정은 남긴다. 그룹에서 빼는 것까지만 한다.
  id -u "$ACCOUNT" >/dev/null 2>&1 && sudo gpasswd -d "$ACCOUNT" "$GROUP" >/dev/null 2>&1 || true
  echo "account-kept"
fi
echo "ok"
"#,
        account = quote(account),
        group = quote(group),
        ours = if ours { "yes" } else { "no" },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrets_core::server::InstallMode;

    /// 실제로 셸에 돌려 본다. 철자를 맞춰 보는 것으로는 안전하다는 증거가 안 된다.
    fn through_shell(value: &str) -> String {
        let done = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("printf %s {}", quote(value)))
            .output()
            .expect("sh");
        assert!(done.status.success());
        String::from_utf8(done.stdout).unwrap()
    }

    #[test]
    fn values_cannot_break_out_of_their_quotes() {
        // 계정 이름 한 칸으로 서버에서 임의 명령이 돌면 안 된다.
        for nasty in [
            "a'; rm -rf /; echo '",
            "$(whoami)",
            "`id`",
            "a\"b",
            "line1\nline2",
            "back\\slash",
            "deploy",
        ] {
            assert_eq!(
                through_shell(nasty),
                nasty,
                "{nasty:?} 가 그대로 전달돼야 한다"
            );
        }
    }

    #[test]
    fn a_user_account_gets_no_sudoers_file() {
        let text = install(
            "deploy",
            "user",
            "/srv",
            "workspace",
            "ssh-ed25519 AAAA",
            InstallMode::New,
        );
        assert!(text.contains("sudo rm -f \"$RULE\""));
        assert!(
            !text.contains("NOPASSWD:ALL\\n' \"$ACCOUNT\" > \"$T\"\nsudo install"),
            "사용자에게 규칙을 쓰면 안 된다"
        );
    }

    #[test]
    fn creating_an_existing_login_stops_before_any_server_write() {
        let mock = "id() { [ \"$1\" = -u ] && [ \"$2\" = deploy ]; }\ngetent() { printf 'deploy:x:1001:1001::/home/deploy:/bin/bash\\n'; }\nsudo() { echo sudo-called >&2; exit 99; }\n";
        let done = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(
                "{mock}{}",
                install(
                    "deploy",
                    "user",
                    "/srv",
                    "workspace",
                    "ssh-ed25519 AAAA",
                    InstallMode::New
                )
            ))
            .output()
            .unwrap();
        let error = String::from_utf8_lossy(&done.stderr);
        assert!(!done.status.success());
        assert!(error.contains("account-taken"), "{error}");
        assert!(!error.contains("sudo-called"), "{error}");

        let retry = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(
                "{mock}{}",
                install(
                    "deploy",
                    "user",
                    "/srv",
                    "workspace",
                    "ssh-ed25519 AAAA",
                    InstallMode::Reinstall
                )
            ))
            .output()
            .unwrap();
        let error = String::from_utf8_lossy(&retry.stderr);
        assert!(error.contains("sudo-called"), "{error}");
        assert!(!error.contains("account-taken"), "{error}");
    }

    #[test]
    fn a_sudoers_file_is_always_checked_before_it_is_installed() {
        let text = install(
            "admin",
            "admin",
            "/srv",
            "workspace",
            "ssh-ed25519 AAAA",
            InstallMode::New,
        );
        let checked = text.find("visudo -c").expect("검사가 있어야 한다");
        let installed = text.find("install -o root -g root -m 440").expect("설치");
        assert!(checked < installed, "검사가 설치보다 먼저여야 한다");
    }

    #[test]
    fn every_script_parses_in_sh() {
        for text in [
            inspect(),
            prepare(),
            install(
                "admin",
                "admin",
                "/srv",
                "workspace",
                "ssh-ed25519 AAAA",
                InstallMode::New,
            ),
            install(
                "admin",
                "admin",
                "/srv",
                "workspace",
                "ssh-ed25519 AAAA",
                InstallMode::Reinstall,
            ),
            remove("admin", "workspace", true),
        ] {
            let done = std::process::Command::new("sh")
                .args(["-n", "-c", &text])
                .output()
                .expect("sh");
            assert!(
                done.status.success(),
                "{}",
                String::from_utf8_lossy(&done.stderr)
            );
        }
    }

    #[test]
    fn an_existing_group_of_the_same_name_becomes_the_primary_group() {
        let text = install(
            "admin",
            "admin",
            "/srv",
            "workspace",
            "ssh-ed25519 AAAA",
            InstallMode::New,
        );
        let seen = text
            .find("elif getent group \"$ACCOUNT\"")
            .expect("그룹을 먼저 봐야 한다");
        let reused = text
            .find("useradd -m -s /bin/bash -g \"$ACCOUNT\"")
            .expect("있는 그룹을 쓴다");
        let fresh = text
            .find("useradd -m -s /bin/bash -U")
            .expect("없으면 새로 만든다");
        assert!(seen < reused && reused < fresh);
    }

    #[test]
    fn a_user_role_cannot_take_a_group_that_grants_sudo() {
        let text = install(
            "sudo",
            "user",
            "/srv",
            "workspace",
            "ssh-ed25519 AAAA",
            InstallMode::New,
        );
        let refused = text.find("group-grants-sudo").expect("막아야 한다");
        let reused = text.find("-g \"$ACCOUNT\" \"$ACCOUNT\"").expect("재사용");
        assert!(refused < reused, "재사용보다 먼저 막아야 한다");
        assert!(text.contains(r#"[ "$ROLE" != admin ]"#));
    }

    #[test]
    fn removing_an_account_restores_a_system_group_userdel_took_with_it() {
        let text = remove("admin", "workspace", true);
        let noted = text
            .find("GID=$(getent group")
            .expect("gid 를 먼저 적어 둔다");
        let deleted = text.find("userdel -r").expect("userdel");
        let restored = text.find("groupadd -g \"$GID\"").expect("되살린다");
        assert!(noted < deleted && deleted < restored);
    }

    #[test]
    fn only_our_own_line_is_taken_out_of_authorized_keys() {
        let text = install(
            "deploy",
            "user",
            "/srv",
            "workspace",
            "ssh-ed25519 AAAA",
            InstallMode::New,
        );
        assert!(
            text.contains(r#"grep -v -- " $MARK$""#),
            "표시가 붙은 줄만 지워야 한다"
        );
    }

    #[test]
    fn an_account_we_did_not_create_is_not_deleted() {
        let kept = remove("ubuntu", "workspace", false);
        assert!(kept.contains(r#"[ "no" = "yes" ]"#), "{kept}");
        let ours = remove("deploy", "workspace", true);
        assert!(ours.contains(r#"[ "yes" = "yes" ]"#));
    }
}
