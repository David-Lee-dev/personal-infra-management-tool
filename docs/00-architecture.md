# 계층과 그 경계를 지키는 장치

의존은 아래로만 흐른다. 화살표는 "누가 누구를 import 하는가" 다.

```
        secrets-gui              secrets-cli          app — 구현 선택과 배선
              └────────────┬───────────┘
                    secrets-local                    outbound — CLI·파일·심링크·시계
                           │
                     secrets-core                    도메인 + 소비자 소유 포트
```

`secrets-core/Cargo.toml` 에 `secrets-local` 이 없다. 그래서 core 는 어떤 어댑터도
부를 수 없고, 포트를 구현하는 쪽은 언제나 바깥이다.

## core 가 프로세스와 파일을 만지지 않는다는 것

의존성 그래프만으로는 부족하다. `std::process` 와 `std::fs` 는 의존성 없이도 쓸 수 있다.
그래서 `crates/secrets-core/clippy.toml` 이 이름 해석 뒤의 정의를 보고 막는다.

```sh
cargo clippy -p secrets-core --lib -- -D warnings
```

패키지나 라이브러리 대상이 없으면 이 명령 자체가 실패한다. 검사 대상이 0개인데 통과하는
일은 생기지 않는다. `CLIPPY_CONF_DIR` 을 설정하면 이 규칙이 통째로 덮이므로 쓰지 않는다.

규칙이 실제로 잡는지 확인한 위반:

| 넣어 본 것 | 결과 |
|---|---|
| `std::process::Command::new` | 차단 |
| `std::fs::read_to_string` | 차단 |
| `std::time::SystemTime::now` | 차단 |
| `std::env::var_os` | 차단 |
| `std::path::Path::exists` | 차단 |

같은 규칙이 `secrets-local` 에는 적용되지 않는다. clippy 가 설정을 빌드 중인 패키지의
manifest 디렉토리에서 찾기 때문이다. local 은 `std::fs` 를 정상적으로 쓴다.

## 무엇이 어디에 있는가

묶는 기준은 타입이 아니라 개념이다. `provider/github.rs` 를 열면 GitHub 에 대해 아는
것이 거기 다 있고, `account/` 를 열면 계정이라는 값과 그 규칙만 있다.

```text
secrets-core/
  account/     계정이라는 값 · 만료 판정 · 이름 규칙 · 아카이브 기록
  identity/    관찰한 신원 · 슬러그 규칙 · 같은 계정인가
  credential/  받아 적는 자격 · Secret
  enrollment/  등록·교체·재확인 절차
  port/        accounts · registry · clock · progress
  project/     프로젝트 기록 · 단계 판정 · 이름 규칙 · 런타임 판정 · 환경 변수 파일 역할
  time.rs      날짜 계산 (시계 없음)

secrets-local/
  vault/       ~/.secrets 뿌리 · 경로 · 계정 기록
  retirement.rs 계정을 현역에서 내린다
  cli/         CLI 찾기와 실행 · 도구 표 · 버전
  provider/    form · browser · github · aws · google
  adapter/     포트를 채운 구현
  switching/   전역 링크 · 커밋 신원
  isolation.rs 격리가 실제로 성립하는지 확인
  project/     프로젝트 기록 파일 · 작업 공간(디렉토리 · git init · 스캔) · 근거 파일 읽기

secrets-gui/
  main.rs      Tauri 진입점
  command/     tools · accounts · login · switching · projects
  dto.rs       화면으로 넘기는 표현
  progress.rs  실행 중인 일을 알리는 통로
  wiring.rs    어떤 구현을 쓸지 고르는 한 곳

secrets-gui/ui/
  accounts/    state · rail · detail · form · reissue · challenge · index
  projects/    index · list · detail · create · side · parts
```

## 포트

| 포트 | 묻는 것 | 구현 |
|---|---|---|
| `AccountGateway` | 이 자격이 누구인지 확인해 달라 | `adapter::accounts` |
| `AccountRegistry` | 이 계정을 원자적으로 들여 달라 | `adapter::registry` |
| `Clock` | 지금이 언제인가 | `adapter::clock` |
| `ProgressSink` | 진행 중인 일을 보여 달라 | GUI 의 터미널 패널 |
| `ProjectStore` | 프로젝트 기록을 읽고 새로 써 달라 | `project::FileProjects` |
| `Workspace` | 이 경로에 무엇이 있나, 디렉토리를 만들고 읽어 달라 | `project::LocalWorkspace` |
| `LocalRepository` | 이 레포에 origin · 전용 키를 설정하고 접속을 확인해 달라 | `project::LocalGit` |
| `RepoKeys` | 이 레포의 키를 찾거나 발급해 달라 | `project::VaultRepoKeys` (배포 키 절차를 부른다) |
| `RemoteRepos` | GitHub 에 레포를 만들어 달라 | `project::GhRepos` |

포트는 도메인의 질문을 드러낸다. "이 명령을 이 환경변수로 실행해 달라"가 아니다.
argv·PATH·출력 파싱은 전부 어댑터 안에 있다.

밖에서 읽어 온 문자열을 **해석하는** 일은 어댑터가 하고, 그 값으로 무엇을 **이름 삼을지**
정하는 일은 core 의 `identity` 가 한다. 어댑터가 슬러그까지 만들어 돌려주면 이름 규칙이
provider 수만큼 갈라진다.

## 테스트

| 갈래 | 무엇을 보는가 |
|---|---|
| `secrets-core` 단위 | 만료 판정, 이름 규칙, 날짜 계산 |
| `tests/enrollment.rs` | 등록 절차 자체. 포트의 가짜 구현만 쓴다 |
| `secrets-local` 단위 | CLI 출력 파싱, 격리 환경변수 조립 |
| `tests/cli_contract.rs` | PATH 에 심은 가짜 실행 파일로 실제 실행 계약 |
| `tests/registration_contract.rs` | 실제 파일시스템 위에서의 원자성 |

UI 는 번들러를 거치지 않으므로 빌드가 세 가지를 대신 본다 — 파일별 구문(`node --check`),
**모듈 그래프**(없는 모듈이나 없는 export 를 import 하지 않는가), 그리고 조립 지점에
테스트용 코드가 남아 있지 않은가.

가짜 실행 파일은 argv·환경변수·stdin 을 기록한다. "비밀값은 stdin 으로만 간다"와
"계정마다 설정 홈이 갈린다"가 검사되는 자리다.
