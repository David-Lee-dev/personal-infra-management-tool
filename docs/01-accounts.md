# 슬라이스 1 — 마스터 계정 등록

## 왜 계정이 먼저인가

키는 계정이 발급한다. 계정을 모르면 키를 봐도 "이게 어느 조직 것인지" 알 수 없고,
회전할 때 어디 가서 새로 발급받아야 하는지도 알 수 없다.
그래서 계정 레지스트리가 먼저이고, 키는 나중에 계정에 매달린다.

**이 슬라이스는 아무것도 옮기지 않는다.** 기존 `~/.ssh` · `~/.aws` 는 그대로 두고,
"내가 가진 계정이 무엇인가"를 기록하고 살아있는지 확인하는 것까지만 한다.

## 모델

AWS 는 서로 무관한 여러 root 로부터 각각 admin 을 받는 상황이 있다. 따라서
**계정(account)** 과 **그 안에서 내가 쓰는 신원(identity)** 을 분리한다.

- 계정 = 청구와 소유의 단위. AWS account, GitHub 조직 또는 개인 계정, Apple 팀
- 신원 = 그 계정 안에서 내가 인증하는 수단. IAM 사용자, SSO 역할, GitHub 로그인

마스터 계정이란 **자식 자격을 발급할 수 있는 신원**을 뜻한다.
deploy key · IAM 액세스 키 · PAT 는 전부 여기서 파생된다.

## 레이아웃

```
~/.secrets/accounts/<provider>/<slug>/account.toml

accounts/
  aws/
    tuk/account.toml
    nemo-play/account.toml
    unknown-default/account.toml
  github/
    personal/account.toml
```

슬러그는 조직/용도 이름. 계정 ID 나 날짜를 넣지 않는다.

## account.toml

```toml
[account]
slug     = "tuk"
provider = "aws"
display  = "tuk 운영 AWS"
owner    = "external"          # self | external — root 를 내가 쥐고 있는가
note     = "root 는 회사 소유. 나는 admin IAM 사용자만 받음"

[aws]
account_id = "123456789012"
alias      = "tuk-prod"
region     = "ap-northeast-2"

[identity]
kind   = "iam-user"            # iam-user | sso-role | root
name   = "david"
source = "aws-profile:tuk-dev-power"   # 실물을 옮기지 않고 기존 위치를 가리킨다

[capabilities]                 # 이 신원으로 무엇을 발급할 수 있는가
issues = ["iam-access-key"]

[verify]
last_checked = 2026-09-22
last_result  = "ok"
```

GitHub:

```toml
[account]
slug     = "personal"
provider = "github"
display  = "개인 계정 aganga7427"
owner    = "self"

[github]
login = "aganga7427"
type  = "user"                 # user | org

[identity]
kind   = "oauth"               # oauth | pat
source = "gh-cli"              # gh CLI 가 이미 쥐고 있는 토큰을 쓴다

[capabilities]
issues = ["deploy-key", "pat"]
```

`source` 가 이 슬라이스의 핵심이다. 자격을 복사해 오지 않고 **기존 위치를 가리킨다.**
나중 슬라이스에서 실물을 번들로 흡수할 때 이 필드만 바꾸면 된다.

## 명령

### `secrets account add <provider>/<slug>`
대화형으로 필드를 채운다. 가능한 건 자동 추론:

- `--from-aws-profile <name>` — `sts get-caller-identity` 로 account_id · 신원 ARN · kind 추출
- `--from-gh` — `gh api user` 로 login · type 추출

추론된 값을 보여주고 확인받은 뒤 기록한다. `owner` · `note` 는 사람만 답할 수 있으므로 묻는다.

### `secrets account ls [--provider P] [--json]`

```
PROVIDER  SLUG             OWNER     IDENTITY         SOURCE                      VERIFIED
aws       tuk              external  iam-user david   aws-profile:tuk-dev-power   ok  1h
aws       nemo-play        self      iam-user mailer  aws-profile:nemo-play-mail  ok  1h
aws       unknown-default  ?         ?                aws-profile:default         미확인
github    personal         self      oauth            gh-cli                      ok  1h
```

### `secrets account show <ref>`
account.toml 전문 + 마지막 검증 결과. 비밀값은 출력하지 않는다.

### `secrets account verify [<ref>]`
provider 별 실제 호출로 살아있는지 확인하고 `[verify]` 를 갱신한다.

| provider | 검증 |
|---|---|
| aws | `aws sts get-caller-identity --profile <source>` → account_id 대조, ARN 에서 신원 추출 |
| github | `gh api user` → login 대조 |

불일치하면 실패로 기록하고 무엇이 다른지 보여준다. account.toml 을 자동으로 고치지 않는다.

### `secrets account discover`
등록 후보를 찾아 보여준다. `~/.aws/{config,credentials}` 의 프로필, `gh auth status` 의 계정.
이미 등록된 것은 제외. **초기 등록을 여기서 시작한다** — `discover` 로 목록을 보고 하나씩 `add`.

### `secrets account rm <ref>`
레지스트리에서만 제거. 실제 계정이나 자격에는 손대지 않는다.

## 완료 기준

1. `secrets account discover` 가 현재 AWS 프로필 3개와 gh 계정을 후보로 나열한다
2. 각각 `add` 해서 `accounts/` 밑에 account.toml 이 생긴다
3. `secrets account verify` 가 전부 실제 호출로 통과하고, `default` 프로필의 정체가 드러난다
4. `secrets account ls` 가 한 표로 보인다

**이 슬라이스가 `default` 프로필 미결을 해소한다.**

## 다음 슬라이스 (예정, 확정 아님)

- 2 — 계정이 발급한 자격을 번들로 등재 (`source` 를 실물 경로로 전환)
- 3 — `doctor` 진단
- 4 — 회전
- 별도 — `.env` 서버↔로컬 동기화
