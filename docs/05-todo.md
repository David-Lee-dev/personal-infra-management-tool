# 남은 작업

2026-09-24 기준. 이 도구는 비밀값 금고(secret-manager)에서 **인프라 관리 도구
(infra-management-tool)** 로 역할을 키운다. 계정 · 키 · IAM 발급에 더해, 서버에서 앱을
누가 어떻게 돌리는지 · 로그가 어디로 가는지 · 트래픽이 어디로 흐르는지까지 기록하고 바꾼다.

근거 문서: `02-aws-iam.md` (IAM 규칙 · 옛 자격 목록), `03-etc.md` (기타), `04-migration.md`
(연결 정보 · 앱 실행 계정 이전 기록).

---

## 1. 바로 할 것

- [ ] **이 리포 커밋** — IAM · 기타 1단계 · 인스턴스 계정 · 연결(Ghostty) · pem 지문 짝짓기 ·
  여러 계정 만들기가 전부 커밋 전이다. 범위와 메시지를 먼저 정한다.
- [ ] **prod `~/data/admin.db`** (+ `-wal` · `-shm`) 보관 — tuk-admin 의 SQLite 로 보인다.
  7월 이후 기록 없음. 옛 디렉토리 정리 때 빠져 아직 보관하지 않았다.
- [ ] **금고 기록 `tuk / apple-api`** — tukapp-prod · tukapp-dev 소비처를
  `/home/deploy/secrets/AuthKey_XZRTQ6ZPQ8.p8` 로 (기타 화면에서 빼고 다시 추가).
- [ ] **작업 ledger `secret-manager-aws-iam`** — 종료 명세를 만들어 승인을 받고 닫는다.

## 2. AWS IAM 교체

규칙은 `02-aws-iam.md`, 교체 기록은 `02-aws-iam.md` 의 "교체 기록". 2026-09-24 신형 적용까지 끝났다.

- [x] 새 IAM 11개 발급 · 소비처 적용 · 금고에 소비처 기록 (2026-09-24)
- [x] 쓰이지 않던 역할 · 정책 3개 삭제 (2026-09-24) — `dev-tuk-api-server-scheduler-role` ·
  `dev-tuk-api-server-power-only` · `DeveloperBoundary`
- [x] 이 맥 `.env` · `.env.local` 은 로컬 전용 IAM(`tuk-api-local-*` · `tuk-admin-local-s3`)으로 둔다 — 2026-09-24 결정
- [ ] 2026-09-25 확인 — dev DB 02:00 restore(`/var/log/dev-refresh.log`) · prod 03:30 applog 아카이브
  (`/home/deploy/.pm2/applog-archive.log`) 가 새 키로 돌았는지, 새 키들의 마지막 사용 시각
- [ ] **다음 작업** 옛 것 걷기 — 인스턴스 역할 3개(`tuk-api-server-role` · `tukdatabase-{prod,dev}-role`)
  떼기, 옛 사용자 `tuk-api-server-s3-handler` · `tuk-bedrock` 키 비활성화 → 30일 뒤 삭제.
  옛 역할의 마지막 사용이 멈췄는지 먼저 본다(dev DB 역할은 SSM 에이전트가 계속 건드린다)
- [ ] 서버 백업 `/root/env-backup-20260924/`(tukapp-prod · tukapp-dev · tukdb-prod · tukdb-dev),
  이 맥 `~/.secrets/archive/env-backup/20260924/` — 옛 키가 들어 있다. 옛 키를 지운 뒤 지운다
- [ ] 리포 사본 맞추기(배포 스크립트 세션) — `ops/tuk-applog-archive.sh` 와 DB `tuk-log-archive.sh` 의
  원본에 자격 파일 읽기 · `AWS_EC2_METADATA_DISABLED` 추가
- [ ] dev 와 prod 가 같은 S3 버킷 — dev 키로 prod 의 `quiz/pool.json` · `avatars/` 에 쓸 수 있다. 버킷을 나눌지
- [ ] `market-analysis-bedrock` — nemo 에서 돈다. 로컬 기기 단계에서
- [ ] `garden-kim` 콘솔 사용자 — 규칙 대상인지 정한다

## 3. tuk 서버 정리 (dev · prod)

앱 실행 계정은 `deploy` 로 옮겼다(`04-migration.md`). 남은 것:

- [ ] prod `ubuntu` 홈 — `rollback-full.tar.gz` · `metrics-scheduler-archive-*.tar.gz` ·
  `tuk.nginx.bak.20260909*` · `.backfill_2026_q2_done` · `logs.sh` 정리
- [ ] `/root` 백업 — 이전 되돌리기용 `nginx-backup` · `logrotate-backup` · `cron-backup` 와
  5월 nginx 백업 2개. 이전이 안정되면 지운다.
- [ ] prod nginx `dev_shadow` — dev 로의 postback 복제를 끄지 않아 error.log 에 연결 거부가
  쌓인다. 필요할 때까지 `mirror` 두 줄을 끄거나 대상을 `10.0.1.189:4002` 로 (dev ufw 도 열어야 함).
- [ ] tuk-admin — prod 어디서도 돌지 않는다. `/srv/tuk-admin` clone 과 `admin.db` 의 처분을 정한다.
- [ ] dev 디스크 — 로그를 비워 57%. 로그 로테이션이 dev 에는 설정돼 있는지 확인.

## 4. 배포 스크립트 정비 (별도 세션)

지금 prod 는 서버 전용 `/home/deploy/pm2/tuk.config.cjs` 로, dev 는 로컬 브랜치
`migrate/deploy-account` 로 돈다. 리포를 이 모양에 맞춘다.

- [ ] tuk-api-server · tuk-gateway 의 경로 수정 커밋(develop `d2bbbcdd` · `9ecccb4`)을 main 으로
- [ ] tuk-crm-server `deploy/start.sh` 의 `/home/ubuntu/workspace/push` 고정 경로
- [ ] pm2 설정을 리포로 — `.ts` 는 `interpreter: "node"` 필요(pm2 6 은 bun 으로 돌리려 한다),
  push 는 `argv[1]` 로 메인 여부를 봐서 pm2 실행기로 부르면 조용히 끝난다 → node 를 직접 exec
- [ ] back ecosystem 의 `PORT: '4101'` — `.env` 가 override 로 이겨서 죽은 값. 포트는 4102 다.
- [ ] 리포 `ops/tuk-applog-archive.sh` 의 태그를 서버 사본과 같은 `deploy-*` 로
- [ ] back 은 `pnpm-lock.yaml` 이 gitignore — 서버마다 잠금 파일을 손으로 복사해야 했다
- [ ] AI 에이전트 지침에 옛 경로(`/home/ubuntu/workspace/*`)가 있으면 `/srv/<리포>` 로

## 5. 공구경 (gonggugyeong)

- [ ] 앱 실행 계정 이전 — 지금 `gonggugyeong-api@blue` 가 `ubuntu` 로 돈다. tuk 과 같은 절차.
- [ ] 이 서버의 IAM · 배포 키 점검

## 6. 로컬 기기 (AWS 정리 뒤)

- [ ] nemo · nemo-mac — 배포 키 4개 전환(nemo-cowork · nemo-crawler · nemo-proxy ·
  Macro-Analysis), 옛 키(nemo-deploy · nemo-mac · nemo-server) 삭제
- [ ] tuk-scheduler — nemo 에서 git 없이 돈다. 금고의 `tuk-scheduler / deploy` 키는 여기에 쓸 예정
- [ ] 이 맥의 평문 비밀 파일
  - `~/Documents/tuk/david-lee-admin_credentials.csv` — 관리자 콘솔 비밀번호, 644 (급함)
  - `~/Documents/tuk-api-server-s3-handler_accessKeys.csv` — S3 키, 644 (옛 키 폐기 때 함께)
  - `~/workspace/upload-keystore.jks` · `~/Downloads/AuthKey_XZRTQ6ZPQ8.p8` — 금고에 든 것의 사본
  - `~/Documents/psql-tunnel.pem` — Naver Cloud, 쓰지 않음
  - `80_my_projects/.05_market-analysis-backup-20260827/.secrets/`
- [ ] 이 맥의 옛 SSH 키 — `~/.ssh/tuk/{personal,agent}` · `github/github_main` · `nemo-mac` 을
  archive 로, `~/.ssh/config.bak.*` 정리
- [ ] GitHub 계정 키 `main` · `for-old-laptop`, 키체인 `github.com / David-Lee-dev`
- [x] 이 맥의 tuk `.env` 파일들 — 2026-09-24 새 키로 교체 (`02-aws-iam.md` 교체 기록)

## 7. 도구 기능

### 이미 있던 할 일

- [ ] 기타 2단계 — 들이기(해시로 흩어진 사본 찾기 · 옮기기), 보관소로 옮기기
- [ ] IAM 정책 수정 — 지금은 발급 때 한 번 붙이고 끝이다. 금고의 `policy.json` 과 AWS 의 인라인
  정책을 함께 바꾸고, 시뮬레이터 탐침을 다시 돌리고, 바뀐 이력을 남긴다. 권한이 달라지면 이름의
  권한 조각과 어긋날 수 있으니 그때는 새 IAM 을 권한다(규칙 9)
- [ ] IAM 소비처의 변수 이름 — `…ACCESS_KEY_ID` 로 끝나야만 받는다. pgbackrest(`repo1-s3-key`)처럼
  `.env` 가 아닌 설정은 실제 이름을 적을 수 없어 `AWS_ACCESS_KEY_ID` 로 적어 두었다
- [ ] GitHub 에서 주인 없는 키를 지우는 버튼
- [ ] 터미널 칸의 줄 순서 — 작업 스레드의 stderr 가 `cli:end` 뒤에 도착한다

### 인프라 관리로 키울 것 — 이번에 손으로 한 일

이번 이전에서 사람과 에이전트가 손으로 한 일이다. 반복될 것이라 도구가 기록하고 실행할 후보다.

- [ ] **서버 조사** — 계정 · `authorized_keys`(지문 · 주석) · 서버의 GitHub 키와 그 키가 GitHub 의
  어느 배포 키인지 · 리포 원격 · pm2 앱 · 포트 · nginx upstream · cron · logrotate 를 한 화면에
- [ ] **로그인 기록으로 키 사용 확인** — sshd 로그의 지문별 마지막 사용. 옛 키를 걷기 전 근거
- [ ] **authorized_keys 걷기** — 지문 목록으로 빼고, 백업 · 비상용 pem 로그인 확인까지
- [ ] **앱 실행 계정 이전** — 런타임 버전 맞추기, `/srv` clone, `.env` 포트만 바꿔 복사, 빌드 결과
  바이트 비교, 옆에 띄우기 → nginx upstream 넘기기 → 옛 것 정지, 스케줄러는 겹치지 않게 교체
- [ ] **무중단 측정** — 전환 중 1초 간격 탐침과 nginx 5xx 집계
- [ ] **nginx upstream 바꾸기** — 백업은 `sites-enabled` 밖, `nginx -t` 성공일 때만 reload
- [ ] **로그 경로 · 로테이션 · S3 아카이브** — 경로가 바뀌면 logrotate 와 아카이브 태그를 같이
- [ ] **배포 키를 서버 계정에 심기** — 금고의 배포 키를 `deploy` 에 넣고 리포마다 `core.sshCommand`
