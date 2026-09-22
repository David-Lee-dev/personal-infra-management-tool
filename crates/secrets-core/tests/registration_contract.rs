//! 계정 등록의 원자성. 확인된 자격만, 정확히 관찰한 그대로 레지스트리에 들어간다.

mod support;

use secrets_core::account::{self, Provider};
use secrets_core::registration::{self, Draft};
use support::Sandbox;

fn draft(slug: &str) -> Draft {
    Draft {
        slug: slug.to_string(),
        display: "설명".into(),
        note: "메모".into(),
    }
}

fn github_token() -> secrets_core::connect::Values {
    let mut values = std::collections::HashMap::new();
    values.insert("token".to_string(), "ghp_test".to_string());
    values
}

/// 로그인에 성공한 응답. 신원·scope·만료일을 헤더와 본문으로 돌려준다.
const GH_OK: &str = r#"if [ "$1" = "api" ] && [ "$2" = "user" ]; then
  case "$3" in
    -i)
      echo "x-oauth-scopes: repo, admin:public_key"
      echo "github-authentication-token-expiration: 2027-01-31 05:00:00 UTC"
      echo ""
      echo '{"login":"octocat"}'
      ;;
    *)
      printf 'octocat\t583231\t\n'
      ;;
  esac
  exit 0
fi
exit 0
"#;

#[test]
fn a_credential_that_cannot_log_in_never_becomes_an_account() {
    let sandbox = Sandbox::new("reg-login-fails");
    sandbox.install("gh", "echo '인증 실패' 1>&2; exit 1");

    let failed = registration::prepare(Provider::Github, &github_token(), |_, _| {});
    assert!(failed.is_err(), "로그인에 실패하면 준비가 끝나면 안 된다");

    assert!(
        !account::exists(Provider::Github, "octocat"),
        "연결하지 못한 자격이 계정으로 남았다"
    );
    assert!(account::list().is_empty(), "레지스트리에 흔적이 남았다");

    let staged: Vec<_> = std::fs::read_dir(sandbox.root().join("tmp"))
        .map(|entries| entries.filter_map(Result::ok).collect())
        .unwrap_or_default();
    assert!(staged.is_empty(), "실패한 자격의 준비 홈이 남았다: {staged:?}");
}

#[test]
fn facts_come_from_the_observation_not_from_the_caller() {
    let _sandbox = Sandbox::new("reg-facts");
    _sandbox.install("gh", GH_OK);

    let (id, probe) = registration::prepare(Provider::Github, &github_token(), |_, _| {}).unwrap();
    assert_eq!(probe.name, "octocat");

    // 사람은 설명만 적는다. 신원·권한·만료일을 적어 넣을 자리가 애초에 없다.
    let account = registration::commit(&id, draft("octocat")).unwrap();

    let stored = account::load(Provider::Github, "octocat").unwrap();
    assert_eq!(stored.identity.name, "octocat");
    assert_eq!(stored.scopes, vec!["repo", "admin:public_key"]);
    assert_eq!(stored.expires.as_deref(), Some("2027-01-31"));
    assert_eq!(stored.note, "메모");
    assert_eq!(stored.slug, account.slug);
    assert!(
        stored.verification.as_ref().is_some_and(|v| v.ok),
        "등록된 계정은 확인을 거친 계정이다"
    );
}

#[test]
fn the_login_from_the_check_becomes_the_accounts_own_cli_home() {
    let _sandbox = Sandbox::new("reg-adopt");
    _sandbox.install(
        "gh",
        &format!("printf '로그인' > \"$GH_CONFIG_DIR/hosts.yml\"\n{GH_OK}"),
    );

    let (id, _) = registration::prepare(Provider::Github, &github_token(), |_, _| {}).unwrap();
    let account = registration::commit(&id, draft("octocat")).unwrap();

    assert_eq!(
        std::fs::read_to_string(account.cli_home().join("hosts.yml")).unwrap(),
        "로그인",
        "확인 때 한 로그인을 그대로 써야 같은 자격으로 두 번 로그인하지 않는다"
    );
}

#[test]
fn committing_onto_an_existing_slug_leaves_that_account_untouched() {
    let _sandbox = Sandbox::new("reg-collision");
    _sandbox.install("gh", GH_OK);

    let (first, _) = registration::prepare(Provider::Github, &github_token(), |_, _| {}).unwrap();
    registration::commit(&first, draft("octocat")).unwrap();
    let before = std::fs::read_to_string(account::dir_of(Provider::Github, "octocat").join("account.toml")).unwrap();

    let (second, _) = registration::prepare(Provider::Github, &github_token(), |_, _| {}).unwrap();
    let mut clash = draft("octocat");
    clash.note = "덮어쓰기 시도".into();
    registration::commit(&second, clash).unwrap_err();

    let after = std::fs::read_to_string(account::dir_of(Provider::Github, "octocat").join("account.toml")).unwrap();
    assert_eq!(before, after, "이미 있는 계정이 덮어써졌다");
}

/// 로그인을 시작만 하고 끝내지 않은 준비는 계정이 될 수 없다.
///
/// 브라우저 로그인은 첫 단계에서 아직 아무도 확인되지 않았다. 여기서 확정을
/// 허용하면 신원 없는 계정이 레지스트리에 들어앉는다.
#[test]
fn a_login_that_was_started_but_not_finished_cannot_be_committed() {
    let _sandbox = Sandbox::new("reg-unchecked");
    _sandbox.install(
        "firebase",
        r#"store="$XDG_CONFIG_HOME/configstore"
mkdir -p "$store"
printf '%s\n' '{"tempLoginState":{"session":"S"}}' > "$store/firebase-tools.json"
echo "https://auth.firebase.tools/login?session=Sx"
exit 0
"#,
    );

    let (id, _) = registration::begin_browser_login(Provider::Firebase, |_, _| {}).unwrap();

    registration::commit(&id, draft("tuk")).unwrap_err();
    assert!(account::list().is_empty(), "확인되지 않은 신원이 계정이 됐다");
}

/// 없는 준비를 가리키는 표로는 아무것도 만들 수 없다.
#[test]
fn an_unknown_preparation_cannot_be_committed() {
    let _sandbox = Sandbox::new("reg-phantom");
    let phantom = secrets_core::registration::PreparationId::named("prep-없는-것");
    registration::commit(&phantom, draft("tuk")).unwrap_err();
    assert!(account::list().is_empty());
}

#[test]
fn discarding_a_preparation_removes_the_credential_it_staged() {
    let sandbox = Sandbox::new("reg-discard");
    sandbox.install(
        "gh",
        &format!("printf '로그인' > \"$GH_CONFIG_DIR/hosts.yml\"\n{GH_OK}"),
    );

    let (id, _) = registration::prepare(Provider::Github, &github_token(), |_, _| {}).unwrap();
    registration::discard(&id);

    registration::commit(&id, draft("octocat")).unwrap_err();
    let staged: Vec<_> = std::fs::read_dir(sandbox.root().join("tmp"))
        .map(|entries| entries.filter_map(Result::ok).collect())
        .unwrap_or_default();
    assert!(staged.is_empty(), "버린 자격이 디스크에 남았다: {staged:?}");
}

/// 두 로그인이 동시에 진행돼도 서로의 세션을 지우지 않는다.
///
/// 준비 홈이 provider 마다 하나뿐이면 나중에 시작한 로그인이 앞선 세션을 지운다.
/// 사용자는 먼저 연 탭의 코드를 넣고 이유 없이 거부당한다.
#[test]
fn two_logins_in_flight_keep_their_own_sessions() {
    let sandbox = Sandbox::new("reg-concurrent");
    sandbox.install(
        "firebase",
        r#"store="$XDG_CONFIG_HOME/configstore"
mkdir -p "$store"
printf '%s\n' '{"tempLoginState":{"session":"S"}}' > "$store/firebase-tools.json"
echo "https://auth.firebase.tools/login?session=Sx"
exit 0
"#,
    );

    let (first, _) = registration::begin_browser_login(Provider::Firebase, |_, _| {}).unwrap();
    let (second, _) = registration::begin_browser_login(Provider::Firebase, |_, _| {}).unwrap();
    assert_ne!(first.as_str(), second.as_str());

    let begun_first = sandbox.call("firebase", 1);
    let begun_second = sandbox.call("firebase", 2);
    assert_ne!(
        begun_first.env.get("XDG_CONFIG_HOME"),
        begun_second.env.get("XDG_CONFIG_HOME"),
        "두 로그인이 같은 설정 홈을 쓰면 나중 것이 앞 세션을 지운다"
    );

    // 첫 번째 세션은 두 번째가 시작된 뒤에도 자기 자리에 그대로 있어야 한다.
    registration::complete_browser_login(&first, "코드", |_, _| {}).ok();
    let exchanged = sandbox.call("firebase", 3);
    assert_eq!(
        exchanged.env.get("XDG_CONFIG_HOME"),
        begun_first.env.get("XDG_CONFIG_HOME"),
        "코드 교환이 자기 로그인의 설정 홈에서 일어나야 한다"
    );
}
