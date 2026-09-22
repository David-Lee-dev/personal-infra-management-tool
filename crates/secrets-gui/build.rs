use std::path::Path;

fn main() {
    // 프론트엔드는 번들러를 거치지 않으므로 구문 오류가 빌드에서 걸리지 않는다.
    // 창이 뜬 뒤에야 "아무것도 동작하지 않는" 형태로 드러나므로 여기서 미리 막는다.
    check_ui_syntax();
    check_ui_entrypoints();
    check_ui_has_no_injected_code();
    tauri_build::build()
}

fn check_ui_syntax() {
    let ui = Path::new("ui");

    // 디렉토리만 등록하면 안 된다. 그 안의 파일을 고쳐도 디렉토리 mtime 은
    // 그대로라, cargo 가 변경을 못 보고 예전 프론트엔드가 박힌 바이너리를
    // 그대로 돌린다. 파일을 하나씩 등록한다.
    println!("cargo:rerun-if-changed=ui");
    if let Ok(entries) = std::fs::read_dir(ui) {
        for entry in entries.flatten() {
            println!("cargo:rerun-if-changed={}", entry.path().display());
        }
    }

    let Some(node) = which("node") else {
        println!("cargo:warning=node 가 없어 UI 구문 검사를 건너뜁니다");
        return;
    };

    // JS 파일이 하나뿐이지만 늘어날 수 있으므로 목록으로 둔다.
    const SCRIPTS: &[&str] = &["app.js"];

    for name in SCRIPTS {
        let path = ui.join(name);
        let output = std::process::Command::new(&node)
            .arg("--check")
            .arg(&path)
            .output();

        match output {
            Ok(result) if !result.status.success() => {
                let stderr = String::from_utf8_lossy(&result.stderr);
                // 첫 줄에 파일·행·원인이 담긴다. 전문을 흘리면 오히려 안 읽힌다.
                let reason = stderr
                    .lines()
                    .find(|l| l.contains("Error") || l.contains("error"))
                    .unwrap_or("구문 오류");
                panic!("{} 구문 오류: {reason}", path.display());
            }
            Err(e) => println!("cargo:warning=UI 구문 검사 실패: {e}"),
            _ => {}
        }
    }
}

/// 테스트용으로 주입한 코드가 남아 있지 않은지 본다.
///
/// 실제로 한 번 사고가 났다 — 주입한 자동 실행 코드와 토큰이 원복되지 않은 채
/// 빌드·커밋되어, 앱이 혼자 계정을 등록했다.
fn check_ui_has_no_injected_code() {
    // 자격으로 보이는 값과, 사람 조작 없이 화면을 움직이는 코드.
    const FORBIDDEN: &[(&str, &str)] = &[
        ("ghp_", "GitHub 토큰"),
        ("github_pat_", "GitHub 토큰"),
        ("AKIA", "AWS 액세스 키"),
        (").click()", "자동 클릭 (테스트 주입 코드)"),
    ];

    let path = Path::new("ui/app.js");
    let Ok(source) = std::fs::read_to_string(path) else {
        return;
    };

    for (needle, what) in FORBIDDEN {
        assert!(
            !source.contains(needle),
            "{}: {what}이(가) 남아 있습니다 (`{needle}`). 테스트 코드를 지우고 다시 빌드하세요",
            path.display()
        );
    }

    // 최상위 마무리 블록은 초기화 호출만 있어야 한다. 화면을 스스로 움직이는
    // 코드가 여기 있으면 주입된 테스트 코드다 — 실제로 두 번 커밋된 적이 있다.
    const DRIVING: &[&str] = &["setTimeout(", "querySelector(", "scrollIntoView"];
    let tail = tail_block(&source);

    for needle in DRIVING {
        assert!(
            !tail.contains(needle),
            "{}: 마무리 블록에 화면을 조작하는 코드가 있습니다 (`{needle}`). 주입한 테스트 코드를 지우세요",
            path.display()
        );
    }

    assert!(
        tail.contains(r#"showTab("env")"#),
        "{}: 기본 탭이 env 가 아닙니다. 테스트용으로 바꿔 둔 것을 되돌리세요",
        path.display()
    );
}

/// 마지막 함수 정의 뒤에 오는 최상위 코드.
fn tail_block(source: &str) -> &str {
    source
        .rfind("\n}\n")
        .map(|i| &source[i..])
        .unwrap_or(source)
}

/// 초기화 호출이 살아 있는지 본다.
///
/// 구문 검사로는 못 잡는 사고가 하나 있다 — 파일 끝이 잘려 나가도 문법은 멀쩡하다.
/// 그러면 창은 뜨는데 아무것도 그려지지 않고, 오류도 나지 않아 원인을 찾기 어렵다.
fn check_ui_entrypoints() {
    const REQUIRED: &[&str] = &["load()", "showTab(", "loadAccounts()"];

    let path = Path::new("ui/app.js");
    let Ok(source) = std::fs::read_to_string(path) else {
        return;
    };

    // 정의가 아니라 호출이 있는지 봐야 하므로 마지막 블록만 확인한다.
    let tail = source
        .rfind("\n}\n")
        .map(|i| &source[i..])
        .unwrap_or(&source);

    for call in REQUIRED {
        assert!(
            tail.contains(call),
            "{}: 초기화 호출 `{call}` 이 없습니다. 파일 끝이 잘렸을 수 있습니다",
            path.display()
        );
    }
}

fn which(binary: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(binary))
        .find(|candidate| candidate.is_file())
}
