# Infmanse

로컬 자격 증명과 프로젝트·서버 연결을 관리하는 macOS 데스크톱 앱입니다. GitHub, AWS 등의 계정과 키를 등록하고, 프로젝트의 환경 변수와 배포에 사용할 자격을 확인할 수 있습니다. 앱 외에 설치된 도구의 상태를 확인하는 `secrets` CLI도 들어 있습니다.

현재 패키징 설정은 **macOS `.app`**을 만듭니다. 아래 절차는 Apple Silicon 또는 Intel Mac에서 소스를 받아 직접 빌드해 설치하는 방법입니다.

## 준비물

- Git 및 Xcode Command Line Tools: `xcode-select --install`로 설치하고, 이미 설치했다면 이 단계는 건너뜁니다.
- [Rust](https://rustup.rs/)의 stable 도구 체인: 이 저장소는 Rust 2024 edition을 사용합니다. Rust 1.85 이상이 필요합니다.
- [Node.js](https://nodejs.org/)와 [Tauri CLI 2](https://v2.tauri.app/start/prerequisites/): 빌드 중 UI JavaScript를 검사하고 앱을 패키징할 때 사용합니다. npm 패키지 설치 단계는 없습니다.
- 저장소에 접근할 수 있는 GitHub 계정: 비공개 저장소라면 클론 전에 인증이 필요합니다.

설치 상태는 다음 명령으로 확인할 수 있습니다.

```sh
xcode-select -p
rustc --version
node --version
git --version
```

Rust를 설치한 직후 `cargo` 명령이 보이지 않으면 새 터미널을 열어 다시 실행하세요. Tauri CLI가 없다면 설치합니다.

```sh
cargo install tauri-cli --version '^2' --locked
cargo tauri --version
```

## 클론 → 패키징 → 설치

```sh
git clone https://github.com/David-Lee-dev/personal-infra-management-tool.git
cd personal-infra-management-tool/crates/secrets-gui
cargo tauri build --bundles app
cd ../..
```

첫 빌드에서는 Rust 의존성을 내려받고 컴파일하므로 시간이 걸립니다. 성공하면 저장소 루트의 `target/release/bundle/macos/Infmanse.app`이 생성됩니다. `--bundles app`은 현재 저장소의 패키징 대상과 같습니다.

Finder에서 `target/release/bundle/macos/Infmanse.app`을 **응용 프로그램** 폴더로 옮기거나, 저장소 루트에서 다음 명령으로 설치합니다.

```sh
ditto target/release/bundle/macos/Infmanse.app /Applications/Infmanse.app
open /Applications/Infmanse.app
```

기존 설치본을 교체하려면 실행 중인 Infmanse를 먼저 종료하세요. macOS가 `/Applications`에 쓰기 권한을 요구하면 Finder에서 앱을 옮겨 설치할 수 있습니다. 이 저장소는 현재 DMG를 만들거나 배포용 코드 서명·공증을 설정하지 않았습니다.

## 첫 실행

1. **도구 상태**에서 사용할 외부 CLI를 확인합니다. 기본 도구는 `gh`, `aws`, `git`, `ssh`입니다. GCP·Firebase 등의 도구는 해당 기능을 쓸 때 필요합니다. 화면에 없는 도구의 설치 방법이 표시됩니다.
2. **자격 증명**에서 필요한 계정과 키를 등록합니다.
3. **프로젝트** 또는 **서버**에서 관리할 대상을 연결합니다.

앱이 관리하는 계정 기록과 키는 기본적으로 `~/.secrets/` 아래에 저장됩니다. 저장소에는 비밀값을 넣지 마세요. 앱을 다시 빌드하거나 교체해도 이 데이터는 별도로 남습니다.

## 선택: `secrets` CLI 설치

데스크톱 앱을 사용하는 데 CLI 설치는 필요하지 않습니다. 터미널에서 외부 도구의 설치 상태를 확인하려면 저장소 루트에서 다음 명령을 실행합니다.

```sh
cargo install --path crates/secrets-cli --locked
secrets tools
```

`secrets tools`는 기본 도구가 없거나 버전 요구 사항을 충족하지 못하면 종료 코드 1을 반환합니다. CLI의 현재 명령은 `tools`뿐이며, 사용법은 `secrets --help`로 볼 수 있습니다.

## 개발과 참고 문서

앱을 패키징하지 않고 실행하려면 `crates/secrets-gui`에서 `cargo tauri dev`를 사용합니다. 코드 구조는 [docs/00-architecture.md](docs/00-architecture.md), 기능별 설계와 현황은 [docs/](docs/)에서 확인할 수 있습니다.
