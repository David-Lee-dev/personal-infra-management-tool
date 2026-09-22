use std::path::Path;

fn main() {
    // 프론트엔드는 번들러를 거치지 않으므로 구문 오류가 빌드에서 걸리지 않는다.
    // 창이 뜬 뒤에야 "아무것도 동작하지 않는" 형태로 드러나므로 여기서 미리 막는다.
    check_ui_syntax();
    tauri_build::build()
}

fn check_ui_syntax() {
    let ui = Path::new("ui");
    println!("cargo:rerun-if-changed=ui");

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

fn which(binary: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(binary))
        .find(|candidate| candidate.is_file())
}
