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
    let mut reports = tools::inspect_all();

    // 버전 명령의 출력은 표에만 쓴다. CLI 에서는 원문을 흘리지 않는다.
    for report in &mut reports {
        let _ = tools::probe_version(report, |_, _| {});
    }

    let width = reports
        .iter()
        .map(|r| r.tool.id.len())
        .max()
        .unwrap_or(4)
        .max("TOOL".len());

    println!("{:<width$}  {:<10}  PATH", "TOOL", "VERSION", width = width);

    for report in &reports {
        let version = match (&report.version, report.found()) {
            (Some(v), _) => v.to_string(),
            (None, true) => "확인 불가".to_string(),
            (None, false) => "-".to_string(),
        };
        let path = match &report.path {
            Some(p) => p.display().to_string(),
            None => format!("없음  ({})", report.tool.install.hint()),
        };
        println!(
            "{:<width$}  {version:<10}  {path}",
            report.tool.id,
            width = width
        );

        // 최소 버전 미달은 경로보다 중요하므로 바로 아래에 이유까지 붙인다.
        if !report.meets_minimum() {
            let minimum = report.tool.minimum.unwrap_or("");
            println!(
                "{:<width$}  {:<10}  ⚠ {minimum} 이상 필요 — {}",
                "",
                "",
                report.tool.minimum_reason,
                width = width
            );
        }
    }

    let found = reports.iter().filter(|r| r.found()).count();
    let blocking: Vec<_> = reports.iter().filter(|r| r.blocks()).collect();

    println!();
    println!("{}개 중 {found}개 설치됨", reports.len());

    if blocking.is_empty() {
        Ok(())
    } else {
        let names: Vec<_> = blocking.iter().map(|r| r.tool.id).collect();
        println!("필수 툴 미충족: {}", names.join(", "));
        std::process::exit(1);
    }
}
