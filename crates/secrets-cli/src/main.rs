use secrets_local::cli::tools;
use secrets_local::isolation;

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

    let verdicts: Vec<_> = reports
        .iter()
        .map(|r| isolation::probe(r, |_, _| {}))
        .collect();

    let width = reports
        .iter()
        .map(|r| r.tool.id.len())
        .max()
        .unwrap_or(4)
        .max("TOOL".len());

    println!("{:<width$}  {:<10}  PATH", "TOOL", "VERSION", width = width);

    for (report, verdict) in reports.iter().zip(&verdicts) {
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

        // 격리는 기본 동작이므로 성립할 때는 아무 말도 하지 않는다.
        // 깨졌을 때만, 왜 이 툴을 쓸 수 없는지 알려준다.
        match verdict.status {
            isolation::Status::Leaked => println!(
                "{:<width$}  {:<10}  ⚠ 계정 격리 불가 — {} ({})",
                "",
                "",
                verdict.evidence,
                verdict.mechanism,
                width = width
            ),
            isolation::Status::Inconclusive => println!(
                "{:<width$}  {:<10}  격리 확인 못 함 — {}",
                "",
                "",
                verdict.evidence,
                width = width
            ),
            _ => {}
        }

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
    let blocking: Vec<_> = reports
        .iter()
        .zip(&verdicts)
        .filter(|(report, verdict)| report.blocks() || isolation::blocks(verdict))
        .map(|(report, _)| report)
        .collect();

    // 격리는 기본 동작이므로, 몇 개가 성립하는지만 한 줄로 확인시킨다.
    let checked = verdicts
        .iter()
        .filter(|v| v.status != isolation::Status::NotApplicable)
        .count();
    let isolated = verdicts
        .iter()
        .filter(|v| v.status == isolation::Status::Isolated)
        .count();

    println!();
    println!("{}개 중 {found}개 설치됨", reports.len());
    println!("계정 격리 {isolated}/{checked} 확인");

    if blocking.is_empty() {
        Ok(())
    } else {
        let names: Vec<_> = blocking.iter().map(|r| r.tool.id).collect();
        println!("필수 툴 미충족: {}", names.join(", "));
        std::process::exit(1);
    }
}
