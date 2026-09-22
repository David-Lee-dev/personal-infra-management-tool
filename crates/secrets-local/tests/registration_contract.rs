//! 계정 등록의 원자성. 확인된 자격만, 정확히 관찰한 그대로 레지스트리에 들어간다.

mod support;

use secrets_core::account::Provider;
use secrets_local::adapter::cli_accounts::{CliAccounts, CredentialStore};
use secrets_local::adapter::file_registry::FileRegistry;
use secrets_local::adapter::system_clock::SystemClock;
use secrets_core::credential::CredentialInput;
use secrets_core::port::{PreparationId, Silent};
use secrets_core::registration::{Draft, Enrollment};
use secrets_core::secret::Secret;
use std::sync::Arc;
use support::Sandbox;

/// 이 머신에 붙는 배선. 게이트웨이와 레지스트리가 같은 보관소를 공유한다.
struct Local {
    gateway: CliAccounts,
    registry: FileRegistry,
    clock: SystemClock,
}

impl Local {
    fn new() -> Local {
        let store = Arc::new(CredentialStore::new());
        Local {
            gateway: CliAccounts::new(store.clone()),
            registry: FileRegistry::new(store),
            clock: SystemClock,
        }
    }

    fn enrollment(&self) -> Enrollment<'_> {
        Enrollment::new(&self.gateway, &self.registry, &self.clock)
    }
}

fn draft(slug: &str) -> Draft {
    Draft {
        slug: slug.to_string(),
        display: "설명".into(),
        note: "메모".into(),
    }
}

fn github_token() -> CredentialInput {
    CredentialInput::Github {
        token: Secret::new("ghp_test"),
    }
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
    let local = Local::new();
    sandbox.install("gh", "echo '인증 실패' 1>&2; exit 1");

    let failed = local.enrollment().check(Provider::Github, github_token(), &Silent);
    assert!(failed.is_err(), "로그인에 실패하면 준비가 끝나면 안 된다");

    assert!(
        !secrets_local::store::exists(Provider::Github, "octocat"),
        "연결하지 못한 자격이 계정으로 남았다"
    );
    assert!(secrets_local::store::list().is_empty(), "레지스트리에 흔적이 남았다");

    let staged: Vec<_> = std::fs::read_dir(sandbox.root().join("tmp"))
        .map(|entries| entries.filter_map(Result::ok).collect())
        .unwrap_or_default();
    assert!(staged.is_empty(), "실패한 자격의 준비 홈이 남았다: {staged:?}");
}

#[test]
fn facts_come_from_the_observation_not_from_the_caller() {
    let _sandbox = Sandbox::new("reg-facts");
    let local = Local::new();
    _sandbox.install("gh", GH_OK);

    let prepared = local.enrollment().check(Provider::Github, github_token(), &Silent).unwrap();
    let (id, probe) = (prepared.id.clone(), prepared.observation.identity.clone());
    assert_eq!(probe.name(), "octocat");

    // 사람은 설명만 적는다. 신원·권한·만료일을 적어 넣을 자리가 애초에 없다.
    let account = local.enrollment().register(&id, draft("octocat")).unwrap();

    let stored = secrets_local::store::load(Provider::Github, "octocat").unwrap();
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
    let local = Local::new();
    _sandbox.install(
        "gh",
        &format!("printf '로그인' > \"$GH_CONFIG_DIR/hosts.yml\"\n{GH_OK}"),
    );

    let id = local.enrollment().check(Provider::Github, github_token(), &Silent).unwrap().id;
    let account = local.enrollment().register(&id, draft("octocat")).unwrap();

    assert_eq!(
        std::fs::read_to_string(secrets_local::paths::cli_home(&account).join("hosts.yml")).unwrap(),
        "로그인",
        "확인 때 한 로그인을 그대로 써야 같은 자격으로 두 번 로그인하지 않는다"
    );
}

#[test]
fn committing_onto_an_existing_slug_leaves_that_account_untouched() {
    let _sandbox = Sandbox::new("reg-collision");
    let local = Local::new();
    _sandbox.install("gh", GH_OK);

    let first = local.enrollment().check(Provider::Github, github_token(), &Silent).unwrap().id;
    local.enrollment().register(&first, draft("octocat")).unwrap();
    let before = std::fs::read_to_string(secrets_local::paths::dir_of(Provider::Github, "octocat").join("account.toml")).unwrap();

    let second = local.enrollment().check(Provider::Github, github_token(), &Silent).unwrap().id;
    let mut clash = draft("octocat");
    clash.note = "덮어쓰기 시도".into();
    local.enrollment().register(&second, clash).unwrap_err();

    let after = std::fs::read_to_string(secrets_local::paths::dir_of(Provider::Github, "octocat").join("account.toml")).unwrap();
    assert_eq!(before, after, "이미 있는 계정이 덮어써졌다");
}

/// 로그인을 시작만 하고 끝내지 않은 준비는 계정이 될 수 없다.
///
/// 브라우저 로그인은 첫 단계에서 아직 아무도 확인되지 않았다. 여기서 확정을
/// 허용하면 신원 없는 계정이 레지스트리에 들어앉는다.
#[test]
fn a_login_that_was_started_but_not_finished_cannot_be_committed() {
    let _sandbox = Sandbox::new("reg-unchecked");
    let local = Local::new();
    _sandbox.install(
        "firebase",
        r#"store="$XDG_CONFIG_HOME/configstore"
mkdir -p "$store"
printf '%s\n' '{"tempLoginState":{"session":"S"}}' > "$store/firebase-tools.json"
echo "https://auth.firebase.tools/login?session=Sx"
exit 0
"#,
    );

    let (id, _) = local.enrollment().begin_browser_login(Provider::Firebase, &Silent).unwrap();

    local.enrollment().register(&id, draft("tuk")).unwrap_err();
    assert!(secrets_local::store::list().is_empty(), "확인되지 않은 신원이 계정이 됐다");
}

/// 없는 준비를 가리키는 표로는 아무것도 만들 수 없다.
#[test]
fn an_unknown_preparation_cannot_be_committed() {
    let _sandbox = Sandbox::new("reg-phantom");
    let local = Local::new();
    let phantom = PreparationId::named("prep-없는-것");
    local.enrollment().register(&phantom, draft("tuk")).unwrap_err();
    assert!(secrets_local::store::list().is_empty());
}

#[test]
fn discarding_a_preparation_removes_the_credential_it_staged() {
    let sandbox = Sandbox::new("reg-discard");
    let local = Local::new();
    sandbox.install(
        "gh",
        &format!("printf '로그인' > \"$GH_CONFIG_DIR/hosts.yml\"\n{GH_OK}"),
    );

    let id = local.enrollment().check(Provider::Github, github_token(), &Silent).unwrap().id;
    local.enrollment().discard(&id);

    local.enrollment().register(&id, draft("octocat")).unwrap_err();
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
    let local = Local::new();
    sandbox.install(
        "firebase",
        r#"store="$XDG_CONFIG_HOME/configstore"
mkdir -p "$store"
printf '%s\n' '{"tempLoginState":{"session":"S"}}' > "$store/firebase-tools.json"
echo "https://auth.firebase.tools/login?session=Sx"
exit 0
"#,
    );

    let (first, _) = local.enrollment().begin_browser_login(Provider::Firebase, &Silent).unwrap();
    let (second, _) = local.enrollment().begin_browser_login(Provider::Firebase, &Silent).unwrap();
    assert_ne!(first.as_str(), second.as_str());

    let begun_first = sandbox.call("firebase", 1);
    let begun_second = sandbox.call("firebase", 2);
    assert_ne!(
        begun_first.env.get("XDG_CONFIG_HOME"),
        begun_second.env.get("XDG_CONFIG_HOME"),
        "두 로그인이 같은 설정 홈을 쓰면 나중 것이 앞 세션을 지운다"
    );

    // 첫 번째 세션은 두 번째가 시작된 뒤에도 자기 자리에 그대로 있어야 한다.
    local.enrollment().complete_browser_login(&first, &Secret::new("코드"), &Silent).ok();
    let exchanged = sandbox.call("firebase", 3);
    assert_eq!(
        exchanged.env.get("XDG_CONFIG_HOME"),
        begun_first.env.get("XDG_CONFIG_HOME"),
        "코드 교환이 자기 로그인의 설정 홈에서 일어나야 한다"
    );
}

/// 실패한 교체는 계정을 한 바이트도 바꾸지 않는다.
#[test]
fn a_replacement_that_is_refused_changes_nothing() {
    let sandbox = Sandbox::new("replace-refused");
    let local = Local::new();
    sandbox.install("gh", GH_OK);

    let first = local.enrollment().check(Provider::Github, github_token(), &Silent).unwrap().id;
    let account = local.enrollment().register(&first, draft("octocat")).unwrap();
    std::fs::write(secrets_local::paths::cli_home(&account).join("hosts.yml"), "원래-자격").unwrap();

    let toml = secrets_local::paths::dir_of(Provider::Github, "octocat").join("account.toml");
    let before = std::fs::read_to_string(&toml).unwrap();

    // 다른 계정의 자격을 넣는다.
    sandbox.install(
        "gh",
        GH_OK.replace("octocat", "someone-else").as_str(),
    );
    let other = local.enrollment().check(Provider::Github, github_token(), &Silent).unwrap().id;
    local.enrollment().reissue(&account, &other).unwrap_err();

    assert_eq!(std::fs::read_to_string(&toml).unwrap(), before, "계정 기록이 바뀌었다");
    assert_eq!(
        std::fs::read_to_string(secrets_local::paths::cli_home(&account).join("hosts.yml")).unwrap(),
        "원래-자격",
        "쓰던 자격이 훼손됐다"
    );
    assert!(secrets_local::store::history(&account).is_empty(), "붙지 못한 자격이 이력에 남았다");
}

/// 성공한 교체는 이력을 정확히 하나 남기고 새 자격을 제자리에 놓는다.
#[test]
fn a_replacement_that_succeeds_records_exactly_one_entry() {
    let sandbox = Sandbox::new("replace-ok");
    let local = Local::new();
    sandbox.install(
        "gh",
        &format!("printf '첫-자격' > \"$GH_CONFIG_DIR/hosts.yml\"\n{GH_OK}"),
    );

    let first = local.enrollment().check(Provider::Github, github_token(), &Silent).unwrap().id;
    let account = local.enrollment().register(&first, draft("octocat")).unwrap();

    sandbox.install(
        "gh",
        &format!(
            "printf '새-자격' > \"$GH_CONFIG_DIR/hosts.yml\"\n{}",
            GH_OK.replace("2027-01-31", "2028-06-30")
        ),
    );
    let next = local.enrollment().check(Provider::Github, github_token(), &Silent).unwrap().id;
    let updated = local.enrollment().reissue(&account, &next).unwrap();

    assert_eq!(
        std::fs::read_to_string(secrets_local::paths::cli_home(&account).join("hosts.yml")).unwrap(),
        "새-자격",
        "새 자격이 제자리에 놓이지 않았다"
    );
    assert_eq!(updated.expires.as_deref(), Some("2028-06-30"));
    assert_eq!(
        secrets_local::store::load(Provider::Github, "octocat").unwrap().expires.as_deref(),
        Some("2028-06-30"),
        "교체 결과가 저장되지 않았다"
    );

    let history = secrets_local::store::history(&updated);
    assert_eq!(history.len(), 1, "이력이 정확히 하나여야 한다");
    // 사정은 호출자가 적어 넣는 게 아니라 계정의 만료 상태에서 나온다.
    assert_eq!(history[0].detail, "기한 전 교체");
    assert_eq!(history[0].expires.as_deref(), Some("2027-01-31"), "이력은 구 자격의 것이다");

    assert!(
        !secrets_local::paths::dir(&account).join("cli.replaced").exists(),
        "밀어 둔 옛 자격이 남았다"
    );
}

/// 교체 기록을 남기지 못하면 교체 자체를 되돌린다.
///
/// 새 자격이 붙었는데 이력이 없으면 "언제 무엇을 왜 바꿨나" 가 비는데, 이 도구에서
/// 그 기록은 자격만큼 중요하다. 반쪽짜리로 끝내느니 손대기 전으로 돌아간다.
#[test]
fn a_replacement_that_cannot_be_recorded_is_rolled_back() {
    let sandbox = Sandbox::new("replace-rollback");
    let local = Local::new();
    sandbox.install(
        "gh",
        &format!("printf '원래-자격' > \"$GH_CONFIG_DIR/hosts.yml\"\n{GH_OK}"),
    );

    let first = local.enrollment().check(Provider::Github, github_token(), &Silent).unwrap().id;
    let account = local.enrollment().register(&first, draft("octocat")).unwrap();

    // history 자리를 파일이 차지하고 있으면 기록을 남길 수 없다.
    std::fs::write(secrets_local::paths::dir(&account).join("history"), "").unwrap();

    sandbox.install(
        "gh",
        &format!("printf '새-자격' > \"$GH_CONFIG_DIR/hosts.yml\"\n{GH_OK}"),
    );
    let next = local.enrollment().check(Provider::Github, github_token(), &Silent).unwrap().id;
    let message = local.enrollment().reissue(&account, &next)
        .unwrap_err()
        .to_string();
    assert!(message.contains("되돌"), "되돌렸다는 사실을 말해야 한다: {message}");

    assert_eq!(
        std::fs::read_to_string(secrets_local::paths::cli_home(&account).join("hosts.yml")).unwrap(),
        "원래-자격",
        "쓰던 자격이 돌아오지 않았다"
    );
    assert!(
        !secrets_local::paths::dir(&account).join("cli.replaced").exists(),
        "밀어 둔 자격이 제자리로 돌아가지 않고 남았다"
    );
}

/// 확인 결과를 남기지 못하면 확인은 실패다.
///
/// 조용히 삼키면 화면은 "확인됨"을 보여 주는데 다음에 열면 옛 결과가 그대로 있다.
#[test]
fn a_verification_that_cannot_be_recorded_is_a_failure() {
    let sandbox = Sandbox::new("verify-unsaveable");
    let local = Local::new();
    sandbox.install("gh", GH_OK);

    let id = local.enrollment().check(Provider::Github, github_token(), &Silent).unwrap().id;
    let account = local.enrollment().register(&id, draft("octocat")).unwrap();

    // account.toml 자리를 디렉토리가 차지하면 기록을 쓸 수 없다.
    let record = secrets_local::paths::dir(&account).join("account.toml");
    std::fs::remove_file(&record).unwrap();
    std::fs::create_dir(&record).unwrap();

    local.enrollment().recheck(&account, &Silent).unwrap_err();
}

/// 자격이 거부당한 것과 기록에 실패한 것은 다른 일이다.
#[test]
fn a_rejected_credential_is_recorded_as_a_failed_check() {
    let sandbox = Sandbox::new("verify-rejected");
    let local = Local::new();
    sandbox.install("gh", GH_OK);

    let id = local.enrollment().check(Provider::Github, github_token(), &Silent).unwrap().id;
    let account = local.enrollment().register(&id, draft("octocat")).unwrap();

    sandbox.install("gh", "echo '토큰이 만료됐습니다' 1>&2; exit 1");
    let checked = local.enrollment().recheck(&account, &Silent).unwrap();

    let verification = checked.verification.as_ref().unwrap();
    assert!(!verification.ok, "거부당한 자격이 확인됨으로 남았다");
    assert!(
        !secrets_local::store::load(Provider::Github, "octocat")
            .unwrap()
            .verification
            .unwrap()
            .ok,
        "확인 결과가 저장되지 않았다"
    );
}
