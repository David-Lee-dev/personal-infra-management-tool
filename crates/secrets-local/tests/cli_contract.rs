//! 외부 CLI 와 주고받는 계약. 가짜 실행 파일로 실제 실행 경로를 지나 검사한다.
//!
//! 여기서 고정하는 것은 파싱 결과가 아니라 **무엇을 어떤 환경으로 실행했고 비밀값이
//! 어디로 갔는가**다. 이 계약이 깨지면 자격이 새거나 격리가 무너진다.

mod support;

use secrets_core::account::Provider;
use secrets_local::vault::paths::env_for;
use secrets_local::adapter::accounts::{CliAccounts, CredentialStore};
use secrets_core::port::{AccountGateway, Silent};
use secrets_core::credential::secret::Secret;
use secrets_local::cli::{exec, tools};
use std::sync::{Arc, Mutex};
use support::Sandbox;

/// 관찰자가 받은 줄을 스트림 태그와 함께 모아 둔 것.
type Transcript = Arc<Mutex<Vec<(exec::Stream, String)>>>;

fn collector() -> (Transcript, impl Fn(exec::Stream, String) + Send + Sync + 'static) {
    let lines: Transcript = Arc::new(Mutex::new(Vec::new()));
    let sink = lines.clone();
    (lines, move |stream, line| {
        sink.lock().unwrap().push((stream, line));
    })
}

#[test]
fn path_lookup_finds_the_tool_planted_on_path() {
    let sandbox = Sandbox::new("path-lookup");
    sandbox.install("gh", "exit 0");

    let found = tools::find_in_path("gh").expect("PATH 앞에 심은 gh 를 찾아야 한다");
    assert!(
        found.starts_with(sandbox.root()),
        "실제 gh 가 아니라 심어 둔 것을 찾아야 한다: {}",
        found.display()
    );
}

#[test]
fn a_secret_goes_in_on_stdin_and_never_appears_in_argv() {
    let sandbox = Sandbox::new("stdin-secret");
    sandbox.install("gh", "echo 로그인됨; exit 0");

    let token = "ghp_contract_test_token_value";
    let (lines, on_line) = collector();

    let program = tools::find_in_path("gh").unwrap();
    let outcome = exec::run_full(
        &program,
        &["auth", "login", "--with-token"],
        &[],
        Some(format!("{token}\n").as_bytes()),
        on_line,
    )
    .unwrap();
    assert!(outcome.ok());

    let call = sandbox.last_call("gh");
    assert!(
        !call.argv_contains(token),
        "비밀값이 인자로 나갔다. ps 로 읽힌다: {:?}",
        call.argv
    );
    assert_eq!(call.stdin, format!("{token}\n"), "stdin 으로 정확히 값만 가야 한다");

    let seen = lines.lock().unwrap();
    assert!(
        !seen.iter().any(|(_, line)| line.contains(token)),
        "비밀값이 관찰자에게 흘렀다"
    );
}

#[test]
fn every_provider_gets_its_own_isolated_config_home() {
    let sandbox = Sandbox::new("isolation-env");
    sandbox.install("probe-tool", "exit 0");
    let program = tools::find_in_path("probe-tool").unwrap();

    // provider 마다 격리에 쓰는 환경변수가 다르다. 하나라도 빠지면 그 provider 는
    // 계정별 격리 없이 사용자의 실제 설정을 건드린다.
    let expected: &[(Provider, &[&str])] = &[
        (Provider::Github, &["GH_CONFIG_DIR"]),
        (Provider::Aws, &["AWS_CONFIG_FILE", "AWS_SHARED_CREDENTIALS_FILE"]),
        (Provider::Gcloud, &["CLOUDSDK_CONFIG"]),
        (Provider::Firebase, &["XDG_CONFIG_HOME"]),
    ];

    for (provider, keys) in expected {
        let home = sandbox.root().join("homes").join(provider.id());
        std::fs::create_dir_all(&home).unwrap();

        exec::run_env(&program, &[provider.id()], &env_for(*provider, &home), |_, _| {}).unwrap();

        let call = sandbox.last_call("probe-tool");
        for key in *keys {
            let value = call
                .env
                .get(*key)
                .unwrap_or_else(|| panic!("{} 에 {key} 가 없다", provider.id()));
            assert!(
                value.starts_with(home.to_str().unwrap()),
                "{key} 가 계정 홈을 가리키지 않는다: {value}"
            );
        }
    }
}

#[test]
fn each_line_keeps_the_stream_it_came_from() {
    let sandbox = Sandbox::new("stream-tags");
    sandbox.install("gh", "echo 표준출력; echo 표준오류 1>&2; exit 0");

    let (lines, on_line) = collector();
    let program = tools::find_in_path("gh").unwrap();
    exec::run(&program, &["api", "user"], on_line).unwrap();

    let seen = lines.lock().unwrap();
    assert!(
        seen.contains(&(exec::Stream::Stdout, "표준출력".to_string())),
        "stdout 이 stdout 으로 오지 않았다: {seen:?}"
    );
    assert!(
        seen.contains(&(exec::Stream::Stderr, "표준오류".to_string())),
        "stderr 이 stderr 으로 오지 않았다: {seen:?}"
    );
}

#[test]
fn lines_reach_the_observer_while_the_process_is_still_running() {
    let sandbox = Sandbox::new("live-lines");
    let signal = sandbox.root().join("observer-saw-it");

    // 첫 줄을 내보낸 뒤 관찰자가 그 줄을 받았다는 신호를 기다린다. 출력이 종료
    // 시점에 몰아서 전달되면 신호가 오지 않아 9 로 끝난다.
    sandbox.install(
        "gh",
        &format!(
            r#"echo 시작했습니다
i=0
while [ $i -lt 100 ]; do
  if [ -f "{}" ]; then echo 끝냅니다; exit 0; fi
  sleep 0.05
  i=$((i + 1))
done
exit 9
"#,
            signal.display()
        ),
    );

    let marker = signal.clone();
    let program = tools::find_in_path("gh").unwrap();
    let outcome = exec::run(&program, &["api", "user"], move |_, line| {
        if line.contains("시작했습니다") {
            let _ = std::fs::write(&marker, b"");
        }
    })
    .unwrap();

    assert!(
        outcome.ok(),
        "출력이 프로세스가 끝난 뒤에야 전달됐다 (종료 코드 {:?})",
        outcome.code
    );
}

#[test]
fn the_exit_code_is_reported_as_the_tool_gave_it() {
    let sandbox = Sandbox::new("exit-code");
    sandbox.install("gh", "exit 4");

    let program = tools::find_in_path("gh").unwrap();
    let outcome = exec::run(&program, &["api", "user"], |_, _| {}).unwrap();
    assert_eq!(outcome.code, Some(4));
    assert!(!outcome.ok());
}

/// firebase 로그인은 두 명령이 같은 설정 홈을 공유해야 성립한다.
///
/// 첫 명령이 세션을 configstore 에 남기고, 두 번째 명령이 그 세션으로 코드를 교환한다.
/// 설정 홈이 갈리면 두 번째 명령은 세션을 찾지 못한다.
#[test]
fn firebase_login_is_two_commands_sharing_one_config_home() {
    let sandbox = Sandbox::new("firebase-two-step");

    sandbox.install(
        "firebase",
        r#"store="$XDG_CONFIG_HOME/configstore"
mkdir -p "$store"
case "$1" in
  login)
    case "$2" in
      --no-localhost)
        printf '%s\n' '{"tempLoginState":{"session":"A1B2C"}}' > "$store/firebase-tools.json"
        echo "Visit this URL on any device to log in:"
        echo "https://auth.firebase.tools/login?code_challenge=x&session=A1B2Cdeadbeef"
        echo "session ID:"
        echo "A1B2C"
        exit 0
        ;;
      *)
        grep -q tempLoginState "$store/firebase-tools.json" 2>/dev/null || exit 1
        printf '%s\n' '{"user":{"email":"tuk@tuk.im"}}' > "$store/firebase-tools.json"
        echo "Success! Logged in as tuk@tuk.im"
        exit 0
        ;;
    esac
    ;;
  login:list)
    echo "Logged in as tuk@tuk.im"
    exit 0
    ;;
esac
exit 2
"#,
    );

    let gateway = CliAccounts::new(Arc::new(CredentialStore::new()));
    let (id, challenge) = gateway
        .begin_browser_login(Provider::Firebase, &Silent)
        .expect("로그인을 시작해야 한다");
    assert!(challenge.url.starts_with("https://auth.firebase.tools/login"));
    assert_eq!(challenge.session, "A1B2C", "브라우저에서 대조할 세션 번호를 읽어야 한다");

    let prepared = gateway
        .complete_browser_login(&id, &Secret::new("4/0AXlqoi5-code"), &Silent)
        .expect("같은 설정 홈에서 코드 교환이 끝나야 한다");
    assert_eq!(prepared.observation.identity.name(), "tuk@tuk.im");

    let begin = sandbox.call("firebase", 1);
    let complete = sandbox.call("firebase", 2);
    assert_eq!(begin.argv, vec!["login", "--no-localhost"]);
    assert_eq!(complete.argv, vec!["login", "4/0AXlqoi5-code"]);
    assert_eq!(
        begin.env.get("XDG_CONFIG_HOME"),
        complete.env.get("XDG_CONFIG_HOME"),
        "두 명령이 같은 설정 홈을 써야 세션이 이어진다"
    );
}

/// 끝난 세션에 코드를 넣으면 그 사실을 그대로 말해야 한다.
///
/// firebase 는 코드 시도가 한 번 실패하면 세션을 지운다. 그 뒤의 실패를 "코드가
/// 틀렸다" 로만 보여 주면 사용자는 맞는 코드를 계속 다시 넣는다.
#[test]
fn a_spent_login_session_is_reported_as_spent() {
    let sandbox = Sandbox::new("firebase-spent");

    sandbox.install(
        "firebase",
        r#"store="$XDG_CONFIG_HOME/configstore"
mkdir -p "$store"
if [ "$2" = "--no-localhost" ]; then
  printf '%s\n' '{"tempLoginState":{"session":"A1B2C"}}' > "$store/firebase-tools.json"
  echo "https://auth.firebase.tools/login?session=A1B2Cx"
  exit 0
fi
printf '%s\n' '{}' > "$store/firebase-tools.json"
echo "Authentication Error" 1>&2
exit 1
"#,
    );

    let gateway = CliAccounts::new(Arc::new(CredentialStore::new()));
    let (id, _) = gateway.begin_browser_login(Provider::Firebase, &Silent).unwrap();
    gateway
        .complete_browser_login(&id, &Secret::new("wrong"), &Silent)
        .unwrap_err();

    let message = gateway
        .complete_browser_login(&id, &Secret::new("right"), &Silent)
        .unwrap_err()
        .to_string();
    assert!(
        message.contains("세션"),
        "세션이 소멸했다는 사실을 말해야 한다: {message}"
    );
    assert_eq!(sandbox.call_count("firebase"), 2, "끝난 세션에는 CLI 를 다시 부르지 않는다");
}

/// 코드를 넣는 두 단계 로그인은 firebase 만 쓴다.
///
/// 다른 provider 로 부르면 firebase 를 그 provider 의 격리 설정으로 돌리게 되는데,
/// 그 설정에는 `XDG_CONFIG_HOME` 이 없어 사용자의 실제 firebase 로그인에 닿는다.
#[test]
fn the_two_step_login_refuses_providers_that_do_not_use_it() {
    let sandbox = Sandbox::new("two-step-guard");
    sandbox.install("firebase", "echo '불려서는 안 된다'; exit 0");
    let gateway = CliAccounts::new(Arc::new(CredentialStore::new()));

    for provider in [Provider::Github, Provider::Aws, Provider::Gcloud] {
        gateway
            .begin_browser_login(provider, &Silent)
            .unwrap_err();
    }
    assert_eq!(
        sandbox.call_count("firebase"),
        0,
        "firebase 를 쓰지 않는 provider 인데 firebase 가 실행됐다"
    );
}
