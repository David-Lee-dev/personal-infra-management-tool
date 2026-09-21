# secret-manager 설계

## 문제

자격 증명이 5군데로 파편화되어 있고, 어느 키가 어디에 배포되어 있는지 기록이 없어
회전을 할 수 없다. GitHub 키는 개인 계정 단위 하나(`github_main`)라 전 리포에
동일 권한으로 붙어 있다.

## 원칙

1. `~/.secrets` 가 정본. `~/.ssh` · `~/.aws` 에는 심볼릭 링크와 생성된 설정만 둔다.
2. **분류는 성격(authority) 기준.** 서비스/프로젝트는 경로가 아니라 `meta.toml` 의 태그다.
   같은 성격끼리 모여야 검증·회전 절차를 공유할 수 있다.
3. 번들은 자기완결적. 디렉토리 하나에 키와 그 맥락이 함께 있다.
   중앙 인덱스는 번들 스캔으로 만드는 캐시이며 정본이 아니다.
4. 파일명에 메타데이터를 넣지 않는다. 파일명은 참조 경로이므로 불변이어야 한다.
5. GitHub 접근은 **리포 단위 deploy key** 로 전환한다. 계정 단위 키는 폐기한다.

## 레이아웃

```
~/.secrets/                               # 0700
  ssh/<slug>/                             # 서버 접속 — authority: 각 호스트의 authorized_keys
  github/deploy/<owner>__<repo>/          # authority: GitHub, 리포 1개 범위
  github/token/<slug>/                    # PAT / fine-grained token
  aws/iam/<slug>/                         # 장기 액세스 키 · 프로필
  aws/bedrock/<slug>/                     # 모델 호출 전용 분리 자격
  signing/apple/<slug>/                   # APNs p8, 인증서
  signing/android/<slug>/                 # upload keystore
  token/<slug>/                           # 기타 서드파티

  config/ssh_config.d/*.conf              # 생성물. ~/.ssh/config 가 Include
  config/aws/                             # 생성물. ~/.aws/{config,credentials} 의 소스
  .index.json                             # 스캔 캐시 (비밀값 없음)
  .backup/                                # age 암호화 스냅샷
  audit.log                               # append-only 변경 기록
```

`aws/iam` 과 `aws/bedrock` 을 나누는 이유: bedrock 호출 자격은 애플리케이션이 상시
보유하는 반면 IAM 운영 자격은 사람이 가끔 쓴다. 노출 표면과 회전 주기가 달라서
같은 바구니에 두면 둘 다 느슨한 쪽에 맞춰진다.

## 번들

```
ssh/tuk-prod-deploy/
  key            0600   개인키
  key.pub        0644
  meta.toml      0600   정본 메타데이터
  history/              회전 이력 (구 fingerprint, 폐기일)
```

타입별 실물 파일명 규약 — 참조 경로가 예측 가능해야 한다:

| 타입             | 파일                        |
|------------------|-----------------------------|
| ssh / deploy key | `key`, `key.pub`            |
| aws              | `credentials.toml`          |
| token            | `token`                     |
| apple p8         | `key.p8`                    |
| android keystore | `keystore.jks`              |

## meta.toml

```toml
[identity]
slug    = "tuk-prod-deploy"        # 디렉토리명과 동일, 불변
kind    = "ssh"                    # 상위 경로와 일치
type    = "ssh-ed25519"
purpose = "tuk 운영 서버 배포 및 운영 접속"
tags    = ["tuk", "prod"]          # 서비스/환경은 여기. 경로에 넣지 않는다

[lifecycle]
created           = 2025-07-29
rotate_after_days = 180
status            = "active"       # active | deprecated | revoked

[material]
fingerprint = "SHA256:..."
passphrase  = "keychain"           # keychain | none

[[uses]]                           # 어디에 배포되어 있는가 → 회전 시 교체 대상
target = "tukapp-prod"
kind   = "authorized_keys"
added  = 2025-07-29

[links]                            # 무엇이 이 키를 가리키는가 → 회전 시 갱신 대상
ssh_config = ["tukapp-prod", "tukdb-prod"]
symlinks   = ["~/.ssh/tuk/agent"]
projects   = ["11_tuk"]
```

`[[uses]]` 와 `[links]` 가 이 설계의 핵심이다. 회전 절차를 사람의 기억이 아니라
데이터에서 기계적으로 도출하기 위한 것이다.

## 슬러그 규칙

소문자·하이픈. `<서비스>-<환경>-<역할>` 순, 불필요한 토큰은 생략.
날짜 · `new` · `old` · `final` 금지. 회전해도 슬러그는 유지하고 구버전은 `history/` 로.
deploy key 만 예외로 `<owner>__<repo>` — GitHub 리포와 1:1 이므로 기계 생성한다.

## GitHub deploy key 전환

현재 `github_main` 하나가 모든 리포에 계정 권한으로 붙어 있다. 리포별 deploy key 로
바꾸면 키 하나가 유출돼도 리포 하나로 피해가 갇히고, 리포마다 read-only / write 를
따로 줄 수 있다.

절차 (리포당): 키 생성 → `gh repo deploy-key add` → `~/.ssh/config` 에
`Host github-<repo>` 별칭 추가 → 해당 리모트 URL 을 별칭으로 변경 → 접속 검증.
전 리포 전환 후 계정 키 폐기. 리포 수가 많으므로 이 절차는 CLI 가 일괄 수행한다.

## 기술

- 코어 + CLI: **Rust**. 단일 바이너리, 런타임 없음, 파일 권한과 Keychain 접근이 직접 가능.
- GUI (2차): **Tauri v2**. 비밀값은 Rust 쪽에 머물고 프론트에는 마스킹된 메타데이터만 IPC 로 전달.
- 모든 로직은 `secrets-core` 에. CLI 와 GUI 는 껍데기이며, GUI 없이도 전 기능이 동작해야 한다.

## 단계

1. 번들 포맷 + `secrets import` — 기존 키 흡수, 아무것도 옮기지 않음
2. `secrets doctor` — 권한 · 끊어진 링크 · 접속 가능성 · 회전 기한 진단
3. SSH 키 이전 (kind 별로 한 번에 하나씩, 롤백 가능하게)
4. `ssh_config.d` 분리 + Include
5. GitHub deploy key 일괄 전환, 계정 키 폐기
6. 서명 키 워크스페이스에서 회수, AWS 프로필 이관 및 장기 키 회전
7. age 백업
8. Tauri GUI
