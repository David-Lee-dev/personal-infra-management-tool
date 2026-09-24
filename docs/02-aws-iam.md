# 슬라이스 2 — AWS IAM

## 규칙

모든 AWS 권한은 **최소 권한 IAM 사용자 + 액세스 키**로 부여한다. 시크릿 저장소가 발급하고,
필요한 위치의 `.env`에는 사람이 입력한다. 인스턴스에 역할을 연결하지 않는다 — 역할은
인스턴스 안에 가려져 시크릿 저장소의 기록에서 벗어난다. 모든 자격 증명은 시크릿 저장소에서
발급하고, 어디에 입력했는지는 시크릿 저장소에 기록한다.

1. **IAM 하나에 권한 하나.** 서비스 하나, 범위 하나. 인라인 정책 하나에 리소스를
   명시한다. `*` 리소스와 관리형 정책은 쓰지 않는다 — 연결되지 않은 정책이 남는다.
2. **환경마다 따로.** prod와 dev가 같은 IAM을 쓰지 않는다.
3. **이름은 `<앱>-<환경>-<권한>-iam-<YYYYMMDD>`.** 이름만 보고 누가 어디서 무엇을
   하는지, 언제 만든 세대인지 알 수 있다. 권한 조각은 정책의 서비스다. 같은 앱 · 환경에
   그 조각을 쓰는 IAM이 있으면 대상을 비교한다 — 같으면 새 세대(조각은 그대로, 날짜만
   다름), 다르면 대상 경로의 마지막 부분을 덧붙인다(`s3-applog-archive`). 같은 날 같은
   이름이 다시 나오면 `-2`부터 붙인다.
4. **`.env` 변수 이름에 권한을 표시한다.** `AWS_<권한>_ACCESS_KEY_ID` ·
   `AWS_<권한>_SECRET_ACCESS_KEY`. `.env` 하나에 여러 IAM이 들어갈 수 있다.
5. **상태는 있거나 없거나.** 중간 상태를 화면에 표시하지 않는다.
6. **사용 위치는 기록만 한다.** `.env`에 값을 넣고 빼는 일은 사람이 한다. 시크릿 저장소는
   붙여 넣을 두 줄을 복사해 줄 뿐 파일을 수정하지 않는다. 접근을 차단하는 수단은 파일이 아니라
   IAM 쪽 키다 — 키가 없으면 `.env`에 값이 남아 있어도 사용할 수 없다.
7. **키 교체 기능은 없다.** 새 IAM을 만들고 기존 IAM은 사람이 정리한다. 자동 교체는
   기록에서 누락된 사용 위치를 중단시킨다.
8. **삭제는 키가 30일 넘게 쓰이지 않았을 때만.** 기준은 AWS가 기록한 마지막 사용
   시각이고, 한 번도 쓰이지 않았으면 발급 시각이다. 경계는 31일째부터다 — AWS의
   시각은 UTC 날짜 단위라 하루 차이가 날 수 있다. 시각을 읽지 못하면 삭제하지 않는다.
   사용 위치 기록이 틀려도 아직 쓰이는 키는 삭제되지 않는다.
   **삭제 가능일**을 기록에 둔다. 마지막 사용 시각은 앞으로만 움직이므로 이 날짜는 하한이다 —
   생성할 때 발급일 + 31일로 정하고, AWS에 조회할 때마다(조회 · 거부된 삭제) 다시 기록한다.
   그 전에는 조회하지 않고 삭제를 잠근다.
9. **규칙에 맞지 않는 기존 IAM은 새로 만들어 이전한 뒤 폐기한다.** 기존 IAM을 수정해 맞추지 않는다.

예외: AWS 서비스(Scheduler · Lambda 등)가 사용하는 역할. 키를 받을 수 없어 역할로만 가능하다.
이 도구의 관리 대상이 아니다. 2026-09-23 현재 계정에 해당하는 것은 없다.

마스터 자격 증명(`david-lee-admin`)은 이 규칙의 적용 대상이 아니다. 계정 관리 탭에서 다룬다.

GitHub 배포 키도 같은 이유로 **GitHub 쪽 제목** 끝에 등록일을 붙인다
(`secrets/develop-20260924`). 로컬 경로에는 붙이지 않는다 — 리포의 `core.sshCommand`가
그 경로를 가리킨다. 인스턴스 계정은 리눅스 로그인 이름이라 붙이지 않고, pem은
AWS가 이름을 정한다.

## 이전

| 현재 | 새 이름 |
|---|---|
| `tuk-api-server-s3-handler` | `tuk-api-prod-s3-iam-<날짜>` · `tuk-api-dev-s3-iam-<날짜>` |
| `tuk-bedrock` | `tuk-api-prod-bedrock-iam-<날짜>` · `tuk-api-dev-bedrock-iam-<날짜>` |
| `market-analysis-bedrock` | `market-analysis-prod-bedrock-iam-<날짜>` · `market-analysis-local-bedrock-iam-<날짜>` |
| `tuk-api-server-role` (역할) | 권한을 위 IAM들로 이전한다 |
| `tukdatabase-prod-role` (역할) | `tuk-db-prod-s3-iam-<날짜>` |
| `tukdatabase-dev-role` (역할) | `tuk-db-dev-s3-iam-<날짜>` |

DB 서버는 **마지막에** 이전한다. pgbackrest가 역할을 사용해 S3에 백업을 올리고 있어,
잘못되면 백업이 아무 경고 없이 중단된다.

## 교체 기록 — 2026-09-24

권한은 코드가 실제로 호출하는 것만 부여했다. 발급은 시크릿 저장소의 발급 절차(정책 시뮬레이터 검증 · 키 소유자 확인)를 거쳤고,
적용 후 새 키로 실제 호출을 확인했다. 기존 역할과 기존 키는 아직 유효하다(다음 작업).

| IAM | 권한 | 사용 위치 |
|---|---|---|
| `tuk-api-{prod,dev,local}-s3-iam-20260924` | `tuk-public` 쓰기 `avatars/*` · `reviews/*` · `quiz/pool.json`, 읽기 `remote-config/*` · `home_bottom_banners/current/*` · `quiz/pool.json` | 서버 `/srv/tuk-api-server/.env` · 로컬 `.env.{prod,dev}` · `.env` · `.env.local` |
| `tuk-api-{prod,dev,local}-bedrock-iam-20260924` | quiz 모델 2개(`global.openai.gpt-5.6-luna` · `apac.amazon.nova-pro-v1:0`) InvokeModel | 위와 같음 (`AWS_BEDROCK_*`) |
| `tuk-api-prod-s3-applog-archive-iam-20260924` | `tuk-pgbackrest` `applog-archive/tukapp/*` 읽기 · 쓰기 | tukapp-prod `/home/deploy/.config/tuk-applog-archive.env` |
| `tuk-db-prod-s3-iam-20260924` | `tuk-pgbackrest` `repo/*` 읽기 · 쓰기 · 삭제, 목록은 `repo` 접두사만 | tukdb-prod `/etc/pgbackrest/pgbackrest.conf` (`key-type=shared`) |
| `tuk-db-prod-s3-log-archive-iam-20260924` | `tuk-pgbackrest` `log-archive/*` 읽기 · 쓰기 | tukdb-prod `/var/lib/postgresql/.config/tuk-log-archive.env` |
| `tuk-db-dev-s3-iam-20260924` | `tuk-pgbackrest` `repo/*` 읽기, 목록은 `repo` 접두사만 | tukdb-dev `/etc/pgbackrest/pgbackrest.conf` |
| `tuk-admin-local-s3-iam-20260924` | `tuk-public` `partners/*` · `lucky_tuk/*` 읽기 · 쓰기 | 로컬 `00_tuk-admin/.env.local` |

기존 IAM에서 제외한 권한: S3 버킷 전체 쓰기 · 삭제 · 목록, 코드에서 쓰지 않는 Bedrock 모델 3개(Instagram 분류),
pgbackrest의 버전 권한(`GetObjectVersion` · `DeleteObjectVersion` — 시점 지정 복구를 쓰지 않는다).

아카이브 스크립트 두 개는 자격 증명 파일을 읽고, `AWS_EC2_METADATA_DISABLED=true`로 인스턴스 역할을
대신 사용하지 못하게 했다. 파일이 없으면 스크립트가 중단된다.

검증:
- 앱 — 새 키로 앱과 같은 설정을 읽어 S3 HeadObject · Bedrock Converse 호출. pm2 reload 중 `/health`
  1초 간격 dev 90/90 · prod 120/120 200, 이후 ERROR 로그 0, prod nginx 5xx 0.
- prod DB — `pgbackrest check`로 WAL 000000010000034D00000023 전송, `pg_stat_archiver` 실패 수 변동 없음(9).
- dev DB — `info` · `repo-ls` · `archive.info` 읽기. 실제 restore는 매일 02:00에 실행된다.
- 아카이브 — `sts get-caller-identity`로 호출자가 새 IAM인지 확인, 테스트 객체 쓰기(삭제 후 버전까지 삭제),
  DB 쪽은 DRY_RUN.

롤백: 각 서버 `/root/env-backup-20260924/`의 원본을 제자리에 복원한다. 역할이 아직 연결되어 있어
원본을 복원하면 즉시 이전처럼 동작한다.

### 기존 IAM을 시크릿 저장소에 등록

기존 사용자 3개를 기록으로 등록했다(`origin = "adopted"`, 시크릿 없음). 교체가 끝난
`tuk-api-server-s3-handler` · `tuk-bedrock`은 폐기 예정으로 지정했다. 지정은 기록일 뿐이고,
삭제 기준은 여전히 키의 마지막 사용 시각(규칙 8)이다. `market-analysis-bedrock`은 nemo에서
쓰이고 있어 지정하지 않았다. 역할은 아직 등록할 수 없다.

## 시스템이 완성되면 정리할 것

새 IAM이 사용 위치에 적용되어 동작이 확인된 뒤에 진행한다. 기존 IAM 사용자는 규칙 8과 같은
기준 — 기존 키가 30일 넘게 쓰이지 않음 — 을 확인하고 삭제한다. 삭제 전에 정의를
`~/.secrets/archive/aws/<날짜>/`에 보관하고 근거를 `TOMBSTONE.md`에 남긴다.

### AWS

- [ ] IAM 사용자 폐기 — `tuk-api-server-s3-handler` · `tuk-bedrock` · `market-analysis-bedrock`
- [ ] 인스턴스 역할 연결 해제 후 폐기 — `tuk-api-server-role` · `tukdatabase-prod-role` · `tukdatabase-dev-role` (DB는 마지막)
- [x] `dev-tuk-api-server-scheduler-role` 삭제 — 2026-06-08 생성 후 한 번도 쓰이지 않음. 이 역할을 사용하는 Scheduler 일정 없음. 2026-09-24 삭제
- [x] 연결되지 않은 관리형 정책 `dev-tuk-api-server-power-only` 삭제 — 2026-09-23 `tuk-dev-power` 삭제로 연결 주체 없음. 2026-09-24 삭제
- [x] 연결되지 않은 관리형 정책 `DeveloperBoundary` 삭제 — 연결 · 권한 경계 사용 0. 2026-09-24 삭제 (정의: `archive/aws/2026-09-24/iam-deleted/`)

### 기존 키가 남은 파일

- [ ] `~/Documents/tuk-api-server-s3-handler_accessKeys.csv` — 시크릿 평문, 권한 644, 2026-04-29부터
- [x] tukapp-prod `~/workspace/back/.env.bak` · `.env.bak.202609091949` · `.env.bak.202607061637` — 2026-09-24 기존 운영 디렉토리 전체를 보관한 후 삭제
- [x] tukapp-dev `~/workspace/back/.env.bak.202609091949` · `.env.bak-0821` — 2026-09-24 위와 같음
- [ ] tukapp-prod · tukapp-dev `/srv/*/.env` — 기존 키가 그대로 복사되어 있다. 새 IAM으로 교체되는지 확인
- [ ] 로컬 `11_tuk/02_tuk-api-server/.env` · `.env.dev` · `.env.local` · `.env.prod`, `11_tuk/00_tuk-admin/.env.local` — 새 키로 교체되는지 확인
- [ ] nemo `/srv/infra/workspace/11_tuk/00_tuk-admin/.env.local` — 새 키로 교체되는지 확인

### 서버 · GitHub · 키체인

- [x] gonggugyeong `ubuntu`의 `authorized_keys`에서 `tuk/personal` 줄 제거 — 2026-09-24 서버 5대 전부 (docs/04)
- [ ] 기존 GitHub 배포 키 9개 삭제 — EC2 전환 후. 2026-09-24 tuk 5개 삭제(back · gateway · admin · push · scheduler), nemo 쪽 4개 남음(로컬 서버 단계)
- [ ] 키체인 `github.com / David-Lee-dev` 항목 정리

### 확인이 필요한 것

- [ ] `garden-kim` — 콘솔 사용자, 관리형 정책 3개(EC2 · IAM · S3). 사람 계정이라 규칙 적용 대상인지 정해야 한다
