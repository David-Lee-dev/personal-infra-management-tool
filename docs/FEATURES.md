# 기능 기획 — 키 관리 (초안 v1)

범위: 자격 증명 번들의 생애주기. `.env` 서버↔로컬 동기화는 별도 기능군으로 후속 기획.

## 조직 원리 — plan / apply

`~/.secrets` 의 번들이 **의도**이고, `~/.ssh` · `~/.aws` · 원격 서버 · GitHub 가 **실제**다.
모든 명령은 둘 중 하나로 환원된다.

- 실제를 읽어 의도와의 차이를 보고한다 (`doctor`)
- 의도를 실제에 반영한다 (`apply`)

"어쩌다 이렇게 됐는지 모르겠는 상태"가 다시 생기지 않게 하는 유일한 방법이다.
파괴적 동작은 전부 기본 dry-run 이고 `--apply` 로만 실행한다.

## 참조 표기

`<kind>/<slug>` 가 정식. 유일하면 `<slug>` 만으로도 된다.
예: `ssh/tuk-prod-deploy`, `aws/iam/tuk-dev-power`, `github/deploy/nemo__play-backend`

---

## A. 인벤토리

### `secrets ls [--kind K] [--tag T] [--status S] [--json]`
번들을 스캔해 표로 출력. 컬럼: kind, slug, type, created, 회전까지 남은 일수, uses 개수, status.
회전 기한 초과는 강조. `--json` 은 GUI 와 스크립트용.

### `secrets show <ref>`
meta.toml 전문 + 실물 파일 권한 + fingerprint + `[[uses]]` · `[links]` 전개.
**비밀값은 출력하지 않는다.** 공개키와 fingerprint 만.

### `secrets index`
`.index.json` 캐시 재생성. 다른 명령이 필요 시 자동 호출하므로 보통 직접 쓰지 않는다.

---

## B. 진단

### `secrets doctor [--kind K] [--fix]`
검사 항목:

| 검사 | 내용 |
|---|---|
| 권한 | 번들 0700, 개인키 0600, `~/.secrets` 0700 |
| 무결성 | meta.toml 파싱, 필수 필드, 슬러그와 디렉토리명 일치 |
| 실물 | 선언된 파일 존재, fingerprint 재계산 후 meta 와 대조 |
| 드리프트 | 끊어진 심볼릭 링크, 정본 없는 `~/.ssh` 잔재, 링크 안 된 번들 |
| 생성물 | `ssh_config.d` · `~/.aws/credentials` 가 현재 번들과 일치하는가 |
| 도달성 | ssh `-o BatchMode=yes -T`, aws `sts get-caller-identity`, gh deploy-key 조회 |
| 기한 | `rotate_after_days` 초과, 인증서 만료 임박 |
| 고아 | meta 의 `[[uses]]` 에 없는데 서버 authorized_keys 에 있는 공개키 |

`--fix` 는 안전한 것만 자동 교정한다: 권한, 심볼릭 링크, 생성물 재렌더.
원격 상태 · meta 내용 · 삭제는 절대 자동으로 건드리지 않는다.

도달성 검사는 네트워크를 타므로 `--offline` 으로 건너뛸 수 있다.

---

## C. 반영

### `secrets apply [--dry-run]`
번들로부터 생성물을 렌더링한다. 기본 dry-run, diff 를 보여주고 확인받는다.

- `~/.secrets/config/ssh_config.d/<kind>.conf` — Host 블록
- `~/.ssh/<kind>/<slug>` 심볼릭 링크 (`[links].symlinks`)
- `~/.aws/config` · `~/.aws/credentials` — `aws/*` 번들에서 생성
- `~/.ssh/config` 최상단 `Include ~/.secrets/config/ssh_config.d/*.conf` 보장.
  그 아래 수기 영역은 손대지 않는다.

쓰기 전 원본을 `.bak` 으로 보존하고, ssh 설정은 `ssh -G` 파싱 검증에 실패하면 롤백.
원자적 교체(임시 파일 + rename)만 사용한다.

---

## D. 생성 · 흡수

### `secrets new <kind>/<slug> [--type T] [--tag ...] [--purpose ...]`
번들 생성. ssh 는 ed25519, passphrase 는 생성해서 Keychain 에 저장(기본) 또는 없음.
생성 직후 **배포 안내**를 출력한다 — 공개키 클립보드 복사, 대상별 등록 명령.
`[[uses]]` 는 실제 등록을 확인한 뒤에만 기록된다.

### `secrets import <path> [--kind K] [--slug S]`
기존 파일을 번들로 흡수. 자동 추론:

- `created` ← 파일 mtime
- `fingerprint` ← `ssh-keygen -lf` / `keytool` / 타입별 추출
- `[links].ssh_config` ← `~/.ssh/config` 역파싱 (IdentityFile 이 이 파일을 가리키는 Host)
- `[links].projects` ← 워크스페이스에서 경로 문자열 grep

추론 못 하는 `purpose` · `[[uses]]` 는 비워두고 `doctor` 가 미완성으로 계속 지적한다.
원본은 이동이 아니라 **복사**하고, `secrets adopt <ref>` 로 확인 후에 원본을 링크로 교체한다.

### `secrets import --scan`
`~/.ssh` · `~/.aws` · 워크스페이스를 훑어 흡수 후보를 나열. 대화형으로 골라 일괄 import.
초기 이관에 한 번 쓴다.

---

## E. 회전 — 이 도구의 존재 이유

외부 시스템에 걸친 다단계 절차라 중단·재개가 가능해야 한다.
진행 상태는 `~/.secrets/<ref>/rotation.json` 에 둔다.

### `secrets rotate <ref>`
1. **계획** — `[[uses]]` · `[links]` 를 읽어 "새 키를 넣을 곳"과 "고칠 참조"를 목록화해 보여준다
2. **생성** — 같은 슬러그로 새 실물 생성, 구 실물은 `history/<날짜>/` 로
3. **배포** — 대상마다 등록. ssh 는 `ssh-copy-id`, GitHub 은 `gh`, AWS 는 새 액세스 키 발급
4. **검증** — 대상마다 새 키로 실제 접속 확인. 하나라도 실패하면 여기서 정지
5. **전환** — `apply` 로 참조 갱신
6. **구키 폐기** — 유예기간(기본 7일) 후 대상에서 제거. `secrets rotate --finalize <ref>`

**3~4 사이에는 신·구 키가 공존한다.** 새 키 검증 전에 구키를 지우지 않는 게 원칙이다.
중단 시 `secrets rotate --resume <ref>`, 되돌리려면 `--abort`.

### `secrets rotate --due`
기한 초과 번들 전체를 순차 회전. 각각 확인을 받는다.

---

## F. GitHub 리포 단위 전환

### `secrets github adopt <owner>/<repo>` / `--all`
1. `github/deploy/<owner>__<repo>` 번들 생성
2. `gh repo deploy-key add` (기본 read-only, `--write` 로 쓰기)
3. `Host github-<repo>` 별칭을 `ssh_config.d/github.conf` 에 추가
4. 로컬 클론의 리모트 URL 을 `git@github-<repo>:<owner>/<repo>.git` 로 교체
5. `ssh -T` + `git ls-remote` 로 검증

`--all` 은 워크스페이스의 git 리포를 훑어 GitHub 리모트를 가진 것 전부를 대상으로 한다.
리포별로 read-only / write 를 물어보고, 건너뛴 것은 기록한다.

### `secrets github retire-account-key`
전 리포가 deploy key 로 전환됐는지 확인한 뒤에만 계정 키 폐기를 진행한다.
미전환 리포가 하나라도 남아 있으면 목록을 보여주고 거부한다.

---

## G. 폐기

### `secrets revoke <ref>`
`status = revoked` 로 바꾸고 `[[uses]]` 대상에서 제거를 시도, 생성물에서 제외.
실물은 `~/.secrets/.graveyard/<날짜>/` 로 이동해 유예 보관.

### `secrets purge [--older-than 90d]`
graveyard 정리. 경로 · 크기 · fingerprint 를 tombstone 으로 남긴 뒤 삭제.

---

## H. 백업

### `secrets backup` / `secrets restore <file>`
`age` 로 `~/.secrets` 전체를 단일 암호 파일로. 복호 신원은 Keychain.
`restore` 는 번들 복원 후 `apply` 까지 수행해 새 머신에서 링크 · 설정을 재생성한다.

---

## I. 워크스페이스 감사

### `secrets audit-workspace`
`.env` · 설정 파일에서 고위험 패턴(`AKIA`, `-----BEGIN`, `ghp_`, `sk-`, `xox`)을 스캔.
**값은 출력하지 않고 경로 · 줄번호 · 패턴 종류만.** 번들로 옮길 후보를 제안한다.
`.env` 동기화 기능의 기반이 된다.

---

## 감사 로그

모든 변경 명령은 `~/.secrets/audit.log` 에 append: 시각, 명령, 대상 ref, 결과, 영향받은 외부 대상.
읽기 명령은 기록하지 않는다.

---

## 구현 순서

1. 번들 포맷 + `ls` · `show` · `index`
2. `doctor` (오프라인 검사만)
3. `import` · `import --scan` · `adopt` — 기존 키 흡수
4. `apply` — ssh_config.d + 심볼릭 링크
5. `doctor` 도달성 검사 + aws 생성물
6. `new` · `rotate`
7. `github adopt --all` · `retire-account-key`
8. `revoke` · `purge` · `backup` · `audit-workspace`
9. Tauri GUI

1~4 까지만 가도 "무엇이 어디 있고 어디에 쓰이는가"가 해결된다. 회전은 6 부터.

## 미결

- `~/.aws` 의 `default` 프로필 정체 — 5 단계에서 `sts get-caller-identity` 로 확인
- 1Password / Keychain 을 실물 정본으로 쓸지 — 쓰면 H 가 빠지고 번들은 meta 전용이 된다
