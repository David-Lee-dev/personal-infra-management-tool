# 남은 작업

2026-09-24 기준. 이 도구는 비밀값 저장소(secret-manager)에서 **인프라 관리 도구
(infra-management-tool)** 로 역할을 확장한다. 계정 · 키 · IAM 발급에 더해, 서버에서 앱을
누가 어떻게 실행하는지 · 로그가 어디로 가는지 · 트래픽이 어디로 흐르는지까지 기록하고 변경한다.

근거 문서: `02-aws-iam.md` (IAM 규칙 · 기존 자격 증명 목록), `03-etc.md` (기타), `04-migration.md`
(연결 정보 · 앱 실행 계정 이전 기록).

---

## 1. 바로 할 것

- [ ] **이 리포 커밋** — IAM · 기타 1단계 · 인스턴스 계정 · 연결(Ghostty) · pem 지문 매칭 ·
  여러 계정 생성이 모두 커밋 전이다. 범위와 메시지를 먼저 정한다.
- [ ] **prod `~/data/admin.db`** (+ `-wal` · `-shm`) 보관 — tuk-admin의 SQLite로 보인다.
  7월 이후 기록 없음. 기존 디렉토리 정리 때 누락되어 아직 보관하지 않았다.
- [ ] **시크릿 저장소 기록 `tuk / apple-api`** — tukapp-prod · tukapp-dev 사용 위치를
  `/home/deploy/secrets/AuthKey_XZRTQ6ZPQ8.p8`로 변경 (기타 화면에서 삭제 후 다시 추가).
- [ ] **작업 ledger `secret-manager-aws-iam`** — 종료 명세를 만들어 승인을 받고 닫는다.

## 2. AWS IAM 교체

규칙은 `02-aws-iam.md`, 교체 기록은 `02-aws-iam.md`의 "교체 기록". 2026-09-24 신규 IAM 적용까지 완료했다.

- [x] 새 IAM 11개 발급 · 사용 위치 적용 · 시크릿 저장소에 사용 위치 기록 (2026-09-24)
- [x] 사용하지 않던 역할 · 정책 3개 삭제 (2026-09-24) — `dev-tuk-api-server-scheduler-role` ·
  `dev-tuk-api-server-power-only` · `DeveloperBoundary`
- [x] 로컬 `.env` · `.env.local`은 로컬 전용 IAM(`tuk-api-local-*` · `tuk-admin-local-s3`)으로 유지한다 — 2026-09-24 결정
- [ ] 2026-09-25 확인 — dev DB 02:00 restore(`/var/log/dev-refresh.log`) · prod 03:30 applog 아카이브
  (`/home/deploy/.pm2/applog-archive.log`)가 새 키로 실행되었는지, 새 키들의 마지막 사용 시각
- [ ] **다음 작업** 기존 IAM 정리 — 인스턴스 역할 3개(`tuk-api-server-role` · `tukdatabase-{prod,dev}-role`)
  연결 해제, 기존 사용자 `tuk-api-server-s3-handler` · `tuk-bedrock` 키 비활성화 → 30일 뒤 삭제.
  기존 역할의 마지막 사용이 멈췄는지 먼저 확인한다(dev DB 역할은 SSM 에이전트가 계속 사용한다)
- [ ] 서버 백업 `/root/env-backup-20260924/`(tukapp-prod · tukapp-dev · tukdb-prod · tukdb-dev),
  로컬 `~/.secrets/archive/env-backup/20260924/` — 기존 키가 들어 있다. 기존 키를 삭제한 뒤 함께 삭제한다
- [ ] 리포 사본 동기화(배포 스크립트 세션) — `ops/tuk-applog-archive.sh`와 DB `tuk-log-archive.sh`의
  원본에 자격 증명 파일 읽기 · `AWS_EC2_METADATA_DISABLED` 추가
- [ ] dev와 prod가 같은 S3 버킷을 쓴다 — dev 키로 prod의 `quiz/pool.json` · `avatars/`에 쓸 수 있다. 버킷 분리 여부 결정
- [ ] `market-analysis-bedrock` — nemo에서 실행 중이다. 로컬 기기 단계에서 처리
- [ ] `garden-kim` 콘솔 사용자 — 규칙 적용 대상인지 정한다

## 3. tuk 서버 정리 (dev · prod)

앱 실행 계정은 `deploy`로 이전했다(`04-migration.md`). 남은 작업:

- [ ] prod `ubuntu` 홈 — `rollback-full.tar.gz` · `metrics-scheduler-archive-*.tar.gz` ·
  `tuk.nginx.bak.20260909*` · `.backfill_2026_q2_done` · `logs.sh` 정리
- [ ] `/root` 백업 — 이전 작업 롤백용 `nginx-backup` · `logrotate-backup` · `cron-backup`과
  5월 nginx 백업 2개. 이전이 안정되면 삭제한다.
- [ ] prod nginx `dev_shadow` — dev로의 postback 복제를 끄지 않아 error.log에 연결 거부가
  쌓인다. 필요할 때까지 `mirror` 두 줄을 비활성화하거나 대상을 `10.0.1.189:4002`로 변경 (dev ufw도 열어야 함).
- [ ] tuk-admin — prod 어디에서도 실행되지 않는다. `/srv/tuk-admin` clone과 `admin.db`의 처리 방안을 정한다.
- [ ] dev 디스크 — 로그를 비워 57%. dev에도 로그 로테이션이 설정되어 있는지 확인.

## 4. 배포 스크립트 정비 (별도 세션)

현재 prod는 서버 전용 `/home/deploy/pm2/tuk.config.cjs`로, dev는 로컬 브랜치
`migrate/deploy-account`로 실행된다. 리포를 이 구성에 맞춘다.

- [ ] tuk-api-server · tuk-gateway의 경로 수정 커밋(develop `d2bbbcdd` · `9ecccb4`)을 main에 반영
- [ ] tuk-crm-server `deploy/start.sh`의 `/home/ubuntu/workspace/push` 하드코딩 경로
- [ ] pm2 설정을 리포로 이전 — `.ts`는 `interpreter: "node"` 필요(pm2 6은 bun으로 실행하려 한다),
  push는 `argv[1]`로 메인 여부를 판단하므로 pm2 실행기로 호출하면 오류 없이 종료된다 → node를 직접 exec
- [ ] back ecosystem의 `PORT: '4101'` — `.env`가 override로 우선해 사용되지 않는 값. 포트는 4102다.
- [ ] 리포 `ops/tuk-applog-archive.sh`의 태그를 서버 사본과 같은 `deploy-*`로 변경
- [ ] back은 `pnpm-lock.yaml`이 gitignore 대상 — 서버마다 잠금 파일을 수동으로 복사해야 했다
- [ ] AI 에이전트 지침에 기존 경로(`/home/ubuntu/workspace/*`)가 있으면 `/srv/<리포>`로 변경

## 5. 공구경 (gonggugyeong)

- [ ] 앱 실행 계정 이전 — 현재 `gonggugyeong-api@blue`가 `ubuntu`로 실행 중이다. tuk과 같은 절차.
- [ ] 이 서버의 IAM · 배포 키 점검

## 6. 로컬 기기 (AWS 정리 뒤)

- [ ] nemo · nemo-mac — 배포 키 4개 전환(nemo-cowork · nemo-crawler · nemo-proxy ·
  Macro-Analysis), 기존 키(nemo-deploy · nemo-mac · nemo-server) 삭제
- [ ] tuk-scheduler — nemo에서 git 없이 실행 중이다. 시크릿 저장소의 `tuk-scheduler / deploy` 키는 여기에 사용할 예정
- [ ] 로컬의 평문 시크릿 파일
  - `~/Documents/tuk/david-lee-admin_credentials.csv` — 관리자 콘솔 비밀번호, 644 (긴급)
  - `~/Documents/tuk-api-server-s3-handler_accessKeys.csv` — S3 키, 644 (기존 키 폐기 때 함께 처리)
  - `~/workspace/upload-keystore.jks` · `~/Downloads/AuthKey_XZRTQ6ZPQ8.p8` — 시크릿 저장소에 있는 파일의 사본
  - `~/Documents/psql-tunnel.pem` — Naver Cloud, 사용하지 않음
  - `80_my_projects/.05_market-analysis-backup-20260827/.secrets/`
- [ ] 로컬의 기존 SSH 키 — `~/.ssh/tuk/{personal,agent}` · `github/github_main` · `nemo-mac`을
  archive로 이동, `~/.ssh/config.bak.*` 정리
- [ ] GitHub 계정 키 `main` · `for-old-laptop`, 키체인 `github.com / David-Lee-dev`
- [x] 로컬의 tuk `.env` 파일들 — 2026-09-24 새 키로 교체 (`02-aws-iam.md` 교체 기록)

## 7. 도구 기능

### 이미 있던 할 일

- [ ] 기타 2단계 — 가져오기(해시로 흩어진 사본 찾기 · 이동), 보관소로 이동
- [x] 기존 IAM 등록 · 폐기 예정 지정 (2026-09-24) — 키가 하나이고 관리형 정책이 없는 사용자만 등록한다.
  `tuk-api-server-s3-handler` · `tuk-bedrock`(폐기 예정), `market-analysis-bedrock`을 등록했다
- [ ] 인스턴스 역할도 시크릿 저장소에 등록 — 현재 등록 기능은 사용자만 지원한다. `tuk-api-server-role` ·
  `tukdatabase-{prod,dev}-role`은 미등록 상태라 역할 연결 해제는 수동으로 한다
- [ ] IAM 정책 수정 — 현재는 발급 때 한 번 연결하면 끝이다. 시크릿 저장소의 `policy.json`과 AWS의 인라인
  정책을 함께 변경하고, 정책 시뮬레이터 검증을 다시 실행하고, 변경 이력을 남긴다. 권한이 달라지면 이름의
  권한 조각과 어긋날 수 있으니 그때는 새 IAM 발급을 권장한다(규칙 9)
- [ ] IAM 사용 위치의 변수 이름 — `…ACCESS_KEY_ID`로 끝나는 이름만 입력받는다. pgbackrest(`repo1-s3-key`)처럼
  `.env`가 아닌 설정은 실제 이름을 적을 수 없어 `AWS_ACCESS_KEY_ID`로 적어 두었다
- [ ] GitHub에서 소유자 없는 키를 삭제하는 버튼
- [ ] 터미널 영역의 줄 순서 — 작업 스레드의 stderr가 `cli:end` 뒤에 도착한다

### 인프라 관리 기능으로 확장할 것 — 이번에 수동으로 한 작업

이번 이전 작업에서 사람과 에이전트가 수동으로 처리한 일이다. 반복될 작업이므로 도구가 기록하고 실행할 후보다.

- [ ] **서버 스캔** — 계정 · `authorized_keys`(지문 · 주석) · 서버의 GitHub 키와 그 키가 GitHub의
  어느 배포 키인지 · 리포 원격 · pm2 앱 · 포트 · nginx upstream · cron · logrotate를 한 화면에
- [ ] **로그인 기록으로 키 사용 확인** — sshd 로그의 지문별 마지막 사용. 기존 키를 제거하기 전의 근거
- [ ] **authorized_keys 정리** — 지문 목록으로 삭제하고, 백업 · 비상용 pem 로그인 확인까지
- [ ] **앱 실행 계정 이전** — 런타임 버전 맞추기, `/srv` clone, `.env`의 포트만 바꿔 복사, 빌드 결과
  바이트 비교, 나란히 실행 → nginx upstream 전환 → 기존 프로세스 중지, 스케줄러는 겹치지 않게 교체
- [ ] **무중단 측정** — 전환 중 1초 간격 헬스 체크와 nginx 5xx 집계
- [ ] **nginx upstream 변경** — 백업은 `sites-enabled` 밖, `nginx -t` 성공 시에만 reload
- [ ] **로그 경로 · 로테이션 · S3 아카이브** — 경로가 바뀌면 logrotate와 아카이브 태그를 함께 변경
- [ ] **배포 키를 서버 계정에 배치** — 시크릿 저장소의 배포 키를 `deploy`에 설치하고 리포마다 `core.sshCommand` 지정
