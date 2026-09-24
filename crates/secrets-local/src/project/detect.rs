//! 근거 파일에서 런타임을 찾는다. 파일에 적힌 것만 읽고, 없으면 추측하지 않는다.

use std::path::Path;

use secrets_core::project::{Runtime, RuntimeEvidence};

fn found(runtime: Runtime, version: Option<String>, source: &str) -> RuntimeEvidence {
    RuntimeEvidence {
        runtime,
        version,
        source: source.to_string(),
    }
}

fn read(dir: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(name)).ok()
}

/// 첫 줄. 비어 있으면 없다.
fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
}

/// 파일이 있다는 것만으로 알 수 있는 것.
const PRESENCE: &[(&str, Runtime)] = &[
    ("package.json", Runtime::Node),
    ("pnpm-lock.yaml", Runtime::Pnpm),
    ("yarn.lock", Runtime::Yarn),
    ("package-lock.json", Runtime::Npm),
    ("bun.lockb", Runtime::Bun),
    ("bun.lock", Runtime::Bun),
    ("pyproject.toml", Runtime::Python),
    ("requirements.txt", Runtime::Python),
    ("setup.py", Runtime::Python),
    ("uv.lock", Runtime::Uv),
    ("poetry.lock", Runtime::Poetry),
    ("Cargo.toml", Runtime::Rust),
    ("go.mod", Runtime::Go),
    ("Gemfile", Runtime::Ruby),
    ("pubspec.yaml", Runtime::Dart),
    ("pom.xml", Runtime::Java),
    ("build.gradle", Runtime::Java),
    ("build.gradle.kts", Runtime::Java),
];

/// 첫 줄이 곧 버전인 파일.
const VERSION_FILES: &[(&str, Runtime)] = &[
    (".nvmrc", Runtime::Node),
    (".node-version", Runtime::Node),
    (".python-version", Runtime::Python),
    (".ruby-version", Runtime::Ruby),
    ("rust-toolchain", Runtime::Rust),
];

pub fn runtimes(dir: &Path) -> Vec<RuntimeEvidence> {
    let mut evidence = Vec::new();
    for (name, runtime) in PRESENCE {
        if dir.join(name).is_file() {
            evidence.push(found(*runtime, None, name));
        }
    }
    for (name, runtime) in VERSION_FILES {
        if let Some(version) = read(dir, name).as_deref().and_then(first_line) {
            evidence.push(found(*runtime, Some(version), name));
        }
    }
    evidence.extend(package_manager(dir));
    evidence.extend(rust_versions(dir));
    evidence.extend(go_version(dir));
    evidence.extend(tool_versions(dir));
    evidence
}

/// `package.json` 의 `packageManager` — `pnpm@9.1.0+sha512...`.
fn package_manager(dir: &Path) -> Option<RuntimeEvidence> {
    let text = read(dir, "package.json")?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let field = json.get("packageManager")?.as_str()?;
    let (name, version) = field.split_once('@').unwrap_or((field, ""));
    let runtime = match name {
        "pnpm" => Runtime::Pnpm,
        "yarn" => Runtime::Yarn,
        "npm" => Runtime::Npm,
        "bun" => Runtime::Bun,
        _ => return None,
    };
    let version = version.split('+').next().filter(|v| !v.is_empty()).map(str::to_string);
    Some(found(runtime, version, "package.json packageManager"))
}

/// `Cargo.toml` 의 `rust-version` 과 `rust-toolchain.toml` 의 `channel`.
fn rust_versions(dir: &Path) -> Vec<RuntimeEvidence> {
    let mut evidence = Vec::new();
    let manifest = read(dir, "Cargo.toml").and_then(|t| toml::from_str::<toml::Value>(&t).ok());
    let declared = manifest.as_ref().and_then(|m| {
        m.get("package")
            .and_then(|p| p.get("rust-version"))
            .or_else(|| m.get("workspace")?.get("package")?.get("rust-version"))
            .and_then(|v| v.as_str())
    });
    if let Some(version) = declared {
        evidence.push(found(Runtime::Rust, Some(version.to_string()), "Cargo.toml rust-version"));
    }
    let toolchain = read(dir, "rust-toolchain.toml").and_then(|t| toml::from_str::<toml::Value>(&t).ok());
    if let Some(channel) = toolchain
        .as_ref()
        .and_then(|t| t.get("toolchain")?.get("channel")?.as_str().map(str::to_string))
    {
        evidence.push(found(Runtime::Rust, Some(channel), "rust-toolchain.toml"));
    }
    evidence
}

/// `go.mod` 의 `go 1.22` 줄.
fn go_version(dir: &Path) -> Option<RuntimeEvidence> {
    let text = read(dir, "go.mod")?;
    let version = text
        .lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix("go "))?
        .trim()
        .to_string();
    Some(found(Runtime::Go, Some(version), "go.mod go"))
}

/// asdf · mise 의 `.tool-versions` — `nodejs 22.3.0` 같은 줄.
fn tool_versions(dir: &Path) -> Vec<RuntimeEvidence> {
    let Some(text) = read(dir, ".tool-versions") else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let tool = parts.next()?;
            let version = parts.next()?;
            let runtime = match tool {
                "nodejs" | "node" => Runtime::Node,
                "python" => Runtime::Python,
                "rust" => Runtime::Rust,
                "golang" | "go" => Runtime::Go,
                "ruby" => Runtime::Ruby,
                "pnpm" => Runtime::Pnpm,
                "java" => Runtime::Java,
                _ => return None,
            };
            Some(found(runtime, Some(version.to_string()), ".tool-versions"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::workspace::tests_support::TempDir;

    fn sources(evidence: &[RuntimeEvidence], runtime: Runtime) -> Vec<(String, Option<String>)> {
        evidence
            .iter()
            .filter(|e| e.runtime == runtime)
            .map(|e| (e.source.clone(), e.version.clone()))
            .collect()
    }

    #[test]
    fn a_node_project_reports_each_file_it_read() {
        let dir = TempDir::new("node");
        dir.write(".nvmrc", "v22\n");
        dir.write("package.json", r#"{"packageManager":"pnpm@9.1.0+sha512.abc"}"#);
        dir.write("pnpm-lock.yaml", "");

        let evidence = runtimes(dir.path());

        assert_eq!(
            sources(&evidence, Runtime::Node),
            vec![("package.json".into(), None), (".nvmrc".into(), Some("v22".into()))]
        );
        assert_eq!(
            sources(&evidence, Runtime::Pnpm),
            vec![
                ("pnpm-lock.yaml".into(), None),
                ("package.json packageManager".into(), Some("9.1.0".into()))
            ]
        );
    }

    #[test]
    fn rust_versions_come_from_the_manifest_and_the_toolchain_file() {
        let dir = TempDir::new("rust");
        dir.write("Cargo.toml", "[package]\nname = \"x\"\nrust-version = \"1.79\"\n");
        dir.write("rust-toolchain.toml", "[toolchain]\nchannel = \"1.79.0\"\n");

        let evidence = runtimes(dir.path());

        assert_eq!(
            sources(&evidence, Runtime::Rust),
            vec![
                ("Cargo.toml".into(), None),
                ("Cargo.toml rust-version".into(), Some("1.79".into())),
                ("rust-toolchain.toml".into(), Some("1.79.0".into()))
            ]
        );
    }

    #[test]
    fn go_and_tool_versions_are_read() {
        let dir = TempDir::new("go");
        dir.write("go.mod", "module x\n\ngo 1.22\n");
        dir.write(".tool-versions", "golang 1.22.4\npython 3.12.1\nterraform 1.9\n");

        let evidence = runtimes(dir.path());

        assert!(evidence.contains(&found(Runtime::Go, Some("1.22".into()), "go.mod go")));
        assert!(evidence.contains(&found(Runtime::Go, Some("1.22.4".into()), ".tool-versions")));
        assert!(evidence.contains(&found(Runtime::Python, Some("3.12.1".into()), ".tool-versions")));
        assert_eq!(evidence.len(), 4);
    }

    #[test]
    fn a_directory_without_project_files_reports_nothing() {
        let dir = TempDir::new("empty");
        dir.write("README.md", "# hi");
        assert!(runtimes(dir.path()).is_empty());
    }

    #[test]
    fn a_broken_package_json_still_counts_as_node_but_gives_no_manager() {
        let dir = TempDir::new("broken");
        dir.write("package.json", "{ not json");
        let evidence = runtimes(dir.path());
        assert_eq!(evidence, vec![found(Runtime::Node, None, "package.json")]);
    }
}
