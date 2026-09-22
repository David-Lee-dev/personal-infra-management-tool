# 계층과 그 경계를 지키는 장치

```
        secrets-gui              secrets-cli          app — 구현 선택과 배선
              └────────────┬───────────┘
                     secrets-core                    도메인 + 소비자 소유 포트
                           │
                    secrets-local                    outbound — CLI·파일·심링크·시계
```

참조는 위에서 아래로만 흐른다. `secrets-core/Cargo.toml` 에 `secrets-local` 이 없다는
사실이 그 방향을 컴파일 시점에 강제한다.

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

## 포트

| 포트 | 묻는 것 | 구현 |
|---|---|---|
| `AccountGateway` | 이 자격이 누구인지 확인해 달라 | `adapter::cli_accounts` |
| `AccountRegistry` | 이 계정을 원자적으로 들여 달라 | `adapter::file_registry` |
| `Clock` | 지금이 언제인가 | `adapter::system_clock` |
| `ProgressSink` | 진행 중인 일을 보여 달라 | GUI 의 터미널 패널 |

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

가짜 실행 파일은 argv·환경변수·stdin 을 기록한다. "비밀값은 stdin 으로만 간다"와
"계정마다 설정 홈이 갈린다"가 검사되는 자리다.
