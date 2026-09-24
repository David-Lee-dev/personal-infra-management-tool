use std::path::{Path, PathBuf};

fn main() {
    // 프론트엔드는 번들러를 거치지 않으므로 구문 오류가 빌드에서 걸리지 않는다.
    // 창이 뜬 뒤에야 "아무것도 동작하지 않는" 형태로 드러나므로 여기서 미리 막는다.
    let scripts = ui_scripts();
    check_ui_syntax(&scripts);
    check_ui_module_graph();
    check_ui_bindings();
    check_ui_entrypoints();
    check_ui_has_no_injected_code(&scripts);
    tauri_build::build()
}

/// `ui/` 안의 모든 스크립트.
///
/// 디렉토리만 rerun 대상으로 등록하면 안 된다. 그 안의 파일을 고쳐도 디렉토리
/// mtime 은 그대로라, cargo 가 변경을 못 보고 예전 프론트엔드가 박힌 바이너리를
/// 그대로 돌린다. 파일을 하나씩 등록한다.
fn ui_scripts() -> Vec<PathBuf> {
    println!("cargo:rerun-if-changed=ui");
    println!("cargo:rerun-if-changed=ui-check/module-graph.mjs");
    println!("cargo:rerun-if-changed=ui-check/unbound.mjs");

    let mut scripts = Vec::new();
    collect(Path::new("ui"), &mut scripts);
    scripts.sort();
    scripts
}

fn collect(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        println!("cargo:rerun-if-changed={}", path.display());
        if path.is_dir() {
            collect(&path, found);
        } else if path.extension().is_some_and(|e| e == "js") {
            found.push(path);
        }
    }
}

/// 모듈이 서로를 제대로 가리키는지 본다.
///
/// `node --check` 는 파일 하나의 구문만 본다. 없는 모듈을 import 하거나 있지도 않은
/// 이름을 꺼내 와도 통과한다 — 창이 뜬 뒤에야 빈 화면으로 드러난다.
fn check_ui_module_graph() {
    let Some(node) = which("node") else {
        return;
    };

    let output = std::process::Command::new(&node)
        .arg("ui-check/module-graph.mjs")
        .arg("ui")
        .output();

    match output {
        Ok(result) if !result.status.success() => {
            let reason = String::from_utf8_lossy(&result.stderr);
            panic!("UI 모듈 연결이 끊겼습니다:\n{}", reason.trim());
        }
        Err(e) => println!("cargo:warning=UI 모듈 그래프 검사 실패: {e}"),
        _ => {}
    }
}

/// 다른 모듈의 이름을 import 없이 쓰는지 본다.
///
/// 모듈을 쪼갤 때 참조가 끊겨도 구문과 import 경로는 멀쩡해서, 그 줄이 실제로
/// 실행되기 전까지 드러나지 않는다. 실제로 계정 화면 전체가 이렇게 죽어 있었다.
fn check_ui_bindings() {
    let Some(node) = which("node") else {
        return;
    };

    let output = std::process::Command::new(&node)
        .arg("ui-check/unbound.mjs")
        .arg("ui")
        .output();

    match output {
        Ok(result) if !result.status.success() => {
            let reason = String::from_utf8_lossy(&result.stderr);
            panic!("UI 이름이 묶이지 않았습니다:\n{}", reason.trim());
        }
        Err(e) => println!("cargo:warning=UI 이름 묶임 검사 실패: {e}"),
        _ => {}
    }
}

/// `node --check` 로 각 모듈의 구문을 본다.
///
/// 확장자가 `.js` 면 node 가 CommonJS 로 읽어 `import`/`export` 를 오류로 본다.
/// 검사용 사본만 `.mjs` 로 두어 모듈로 읽히게 한다.
fn check_ui_syntax(scripts: &[PathBuf]) {
    let Some(node) = which("node") else {
        println!("cargo:warning=node 가 없어 UI 구문 검사를 건너뜁니다");
        return;
    };

    let scratch = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_else(|_| ".".into()));

    for path in scripts {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let copy = scratch.join(format!("{name}.mjs"));
        if std::fs::write(&copy, source).is_err() {
            continue;
        }

        let output = std::process::Command::new(&node)
            .arg("--check")
            .arg(&copy)
            .output();
        let _ = std::fs::remove_file(&copy);

        match output {
            Ok(result) if !result.status.success() => {
                let stderr = String::from_utf8_lossy(&result.stderr);
                // 첫 줄에 행·원인이 담긴다. 전문을 흘리면 오히려 안 읽힌다.
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
/// 자격으로 보이는 값과, 사람 조작 없이 화면을 움직이는 코드는 빌드를 막는다.
/// 둘 다 원복을 잊으면 커밋되고, 커밋되면 앱이 혼자 계정을 만든다.
fn check_ui_has_no_injected_code(scripts: &[PathBuf]) {
    const FORBIDDEN: &[(&str, &str)] = &[
        ("ghp_", "GitHub 토큰"),
        ("github_pat_", "GitHub 토큰"),
        ("AKIA", "AWS 액세스 키"),
        (".click()", "자동 클릭 (테스트 주입 코드)"),
    ];

    for path in scripts {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        for (needle, what) in FORBIDDEN {
            assert!(
                !source.contains(needle),
                "{}: {what}이(가) 남아 있습니다 (`{needle}`). 테스트 코드를 지우고 다시 빌드하세요",
                path.display()
            );
        }
    }

    // 조립 지점은 초기화 호출만 있어야 한다. 화면을 스스로 움직이는 코드가 여기
    // 있으면 주입된 테스트 코드다.
    const DRIVING: &[&str] = &["setTimeout(", "querySelector(", "scrollIntoView"];
    let path = Path::new("ui/main.js");
    let Ok(main) = std::fs::read_to_string(path) else {
        return;
    };

    for needle in DRIVING {
        assert!(
            !main.contains(needle),
            "{}: 조립 지점에 화면을 조작하는 코드가 있습니다 (`{needle}`). 주입한 테스트 코드를 지우세요",
            path.display()
        );
    }

    assert!(
        main.contains(r#"showTab("keys")"#),
        "{}: 첫 화면이 자격 증명(keys)이 아닙니다. 테스트용으로 바꿔 둔 것을 되돌리세요",
        path.display()
    );
}

/// 초기화 호출이 살아 있는지 본다.
///
/// 구문 검사로는 못 잡는 사고가 하나 있다 — 파일 끝이 잘려 나가도 문법은 멀쩡하다.
/// 그러면 창은 뜨는데 아무것도 그려지지 않고, 오류도 나지 않아 원인을 찾기 어렵다.
fn check_ui_entrypoints() {
    const REQUIRED: &[&str] = &["loadTools()", "showTab(", "loadAccounts()"];

    let path = Path::new("ui/main.js");
    let Ok(source) = std::fs::read_to_string(path) else {
        panic!("{}: 조립 지점이 없습니다", path.display());
    };

    for call in REQUIRED {
        assert!(
            source.contains(call),
            "{}: 초기화 호출 `{call}` 이 없습니다. 파일 끝이 잘렸을 수 있습니다",
            path.display()
        );
    }
}

fn which(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(binary))
        .find(|candidate| candidate.is_file())
}
