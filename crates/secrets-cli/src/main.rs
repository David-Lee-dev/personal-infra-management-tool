use secrets_core::tools;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("tools") => cmd_tools(),
        Some("--help") | Some("-h") | None => {
            usage();
            Ok(())
        }
        Some(other) => {
            eprintln!("알 수 없는 명령: {other}");
            usage();
            std::process::exit(2);
        }
    }
}

fn usage() {
    println!("secrets {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("사용법:");
    println!("  secrets tools    CLI 설치 상태 점검");
}

fn cmd_tools() -> anyhow::Result<()> {
    let reports = tools::inspect_all();

    let width = reports
        .iter()
        .map(|r| r.tool.id.len())
        .max()
        .unwrap_or(4)
        .max("TOOL".len());

    println!("{:<width$}  PATH", "TOOL", width = width);
    for report in &reports {
        let path = match &report.path {
            Some(p) => p.display().to_string(),
            None => format!("없음  ({})", report.tool.install.hint()),
        };
        println!("{:<width$}  {path}", report.tool.id, width = width);
    }

    let missing = reports.iter().filter(|r| !r.found()).count();
    println!();
    println!(
        "{}개 중 {}개 설치됨",
        reports.len(),
        reports.len() - missing
    );

    Ok(())
}
