# DB 관리

2026-09-27 설계 초안. 아직 구현 전이다.

**범위(사용자 결정 2026-09-27).** DB의 접속 정보와 그 DB가 어디에 올라가 있는지를 기록하고 확인한다.
DDL · DML은 하지 않는다. 역할 · 데이터베이스 만들기, 비밀번호 교체, 백업 · 복원 실행, 쿼리 모두 범위 밖이다.

## 왜

- DB가 이 도구의 어디에도 없다. 코드에도, 스캔에도, 기록에도 보이지 않는다. 그런데 실제로 쓰는 DB는 여럿이다.
  - tuk은 DB 전용 서버(tukdb-prod · tukdb-dev, Postgres)를 쓴다.
  - 앱 서버에서는 redis가 돈다.
  - prod에는 주인이 확실하지 않은 SQLite(`~/data/admin.db`)가 있다.
  - 로컬 개발 DB는 `~/workspace/90_db-volume/`에 있다.
- 지금 "이 환경은 어느 DB에, 어느 계정으로 붙는가"는 `.env` 값 안에만 있다. 이 도구는 값을 읽지 않으므로
  (06-ux 원칙) 그 관계를 알 수 없고, 접속 정보가 여러 `.env`에 흩어져 있어 정본이 없다.

## 원칙

- **DB에 로그인하지 않고 SQL을 보내지 않는다.** 확인은 TCP 연결 한 번이다(SQLite는 파일 메타데이터).
- **올라간 곳은 셋 중 하나다.** 등록된 서버(host는 서버 기록의 주소), 이 기기(`~/workspace/90_db-volume/`),
  외부 관리형(RDS 등, host를 직접 적는다).
- **접속 정보는 이 도구가 정본이다.** 비밀번호를 저장한다(2026-09-27 사용자 결정). `~/.secrets/keys/db/<id>/<계정>/secret`,
  0600. 기록 파일 · 화면 · 작업 로그 · 명령 인자에는 나오지 않는다. 화면에서는 복사만 한다.
- **쓰는 곳은 사람이 적는 기록이다.** 서버 · 로컬 `.env` 값과 비교하지 않는다(2026-09-27 사용자 결정).
- 자동 스캔 · 제안은 하지 않는다. 등록은 사람이 한다.

## 데이터

```text
~/.secrets/databases/<id>.toml            DB 하나 — 올라간 곳 · 접속 정보(값 제외) · 사용 위치
~/.secrets/keys/db/<id>/<사용자>/secret    그 계정의 비밀번호 0600. 사용자가 입력한다
~/.secrets/archive/databases/<시각>-<id>/  등록 해제한 기록
```

```toml
id     = "tuk-db-prod"
name   = "tuk-db-prod"
group  = "툭"
engine = "postgres"                 # postgres | mysql | redis | sqlite | 기타
place  = { kind = "server", server = "tuk-db-server" }
       # server  — 등록된 서버. 확인은 그 서버의 계정으로 SSH
       # local   — 이 기기. 데이터 경로는 ~/workspace/90_db-volume/<프로젝트>-dev/
       # managed — 외부 관리형. host · provider("rds" · "supabase" …)를 적는다
port     = 5432
database = "tuk"                    # sqlite면 path = "…/x.db"
note     = ""
registered_at = "…"

[[users]]                            # 접속 계정. 비밀번호는 keys/db/<id>/<user>/secret
name    = "tuk_app"
purpose = "앱"

[[uses]]                             # 사용 위치 — 어느 환경의 어느 변수가 이 DB를 쓰는가
project  = "tuk-api-server"
environment = "prod"
user     = "tuk_app"
variable = "DATABASE_URL"            # 또는 DB_HOST · DB_PASSWORD처럼 나뉜 변수 여러 개
```

## 연결 확인 (TCP만)

- **이 기기에서** host:port로 TCP 연결을 한 번 연다.
- **쓰는 환경의 서버에서** — 그 환경의 서버 계정으로 SSH에 들어가 같은 TCP 연결을 연다. 같은 서버는 한 번만.
  앱이 실제로 DB에 닿는지는 이쪽이 답한다(DB가 사설망 · `127.0.0.1`에만 열린 경우).
- **SQLite** — 파일 있음 · 크기 · 수정 시각을 읽는다.
- 결과: 열림(지연 ms) · 거부 · 시간 초과 · 이름 풀이 실패, 확인 시각.

## 화면

시안: https://claude.ai/artifact/CuLSRL7oGaGKvcMowRmLG3 (디자인 캔버스 13판, 셋째 줄 — DB 목록 · 상세 · 등록 · 사이드바).

- 왼쪽 메뉴 서버 아래 **DB**. 목록은 그룹별이고, 계정 칩을 누르면 연결 문자열을 복사한다(클립보드는 30초 뒤 지움).
- **DB 상세** — 계정(비밀번호 복사 · 연결 문자열 복사) · 연결 확인 표 · 쓰는 곳 · 정보.
- **DB 등록** — 엔진 · 올라간 곳 · 서버 · 이름 · port · database · 계정과 비밀번호 · 쓰는 곳(선택).
- 서버 상세에 "이 서버의 DB", 프로젝트 환경 카드에 DB 칩.
