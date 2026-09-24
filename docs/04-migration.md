# 옛 연결 정보를 금고 발급분으로 바꾸기

2026-09-24 조사. 서버 5대 · GitHub 리포 20개 · 이 맥의 `~/.ssh` 를 읽기만 해서 모았다.
새 인스턴스 계정 5개(admin · david-admin)로 모든 서버에 들어가지는 것도 이때 확인했다.

## 원칙

- 새 것을 붙이고, 동작을 확인하고, 그다음에 옛 것을 뗀다. 한 번에 한 소비처씩.
- `ubuntu` 의 pem 키는 비상용으로 끝까지 둔다.
- `~/.ssh/config` 는 사람이 고친다. 도구는 바꿀 내용을 diff 로 보여 준다.
- 걷어낸 개인키는 지우지 않고 금고 `archive/` 로 옮긴다.

## 결정 (2026-09-24, 사용자)

1. garden 의 접근(`garden.personal` · `garden.agent`)은 그대로 둔다. 그래서 `ai-agent`
   계정은 남기고, 그 안의 `david.agent` · infra-installer 줄만 걷는다.
2. `infra-installer-nemo-mac` 은 금고의 초기 버전이었다. 모든 서버에서 걷는다.
3. AI 에이전트는 `deploy` 계정을 함께 쓴다. `-ai` Host 는 `deploy` 에 리포가 준비된 뒤 옮긴다.
4. db-prod `ubuntu` 의 `aganga7427@gmail.com` 키는 정체를 모른다. 걷는다.
5. 앱 실행 계정 이전은 이 작업 범위 밖이다. 계정 정보 전환이 끝난 뒤 따로 한다 —
   `deploy` 로 tuk · gonggugyeong 을 추가로 띄우고, 그다음에 `ubuntu` 로 돌던 프로세스를 끈다.
6. cron-scripts 는 인스턴스마다 두지 않고 중앙에서 확인하는 방식으로 바꾼다. 인스턴스에서는
   끄고 걷는다.
7. nemo · nemo-mac 같은 로컬 서버는 이 작업에서 뺀다. AWS 정리가 끝난 뒤 따로 한다.
   그래서 로컬 `~/.ssh/tuk/*` · `github_main` · GitHub 계정 키도 그때 걷는다.

## 옛 연결 → 새 대체

### 이 맥 → 서버

| Host | 지금 | 바꾼 뒤 |
|---|---|---|
| `tukapp-dev` · `tukdb-dev` · `tukapp-prod` · `tukdb-prod` | `ubuntu` + `tuk/personal` | `admin` + 금고 키 |
| `gonggugyeong-prod` | `ubuntu` + `tuk/personal` | `david-admin` + 금고 키 |
| `tukapp-dev-ai` · `tukapp-prod-ai` · `gonggugyeong-prod-ai` | `ai-agent` + `tuk/agent` | `deploy` · `david-deploy` (3단계 뒤) |
| `vpn` | `~/.ssh/lightsail-seoul.pem` (금고로 옮겨져 없음) | 금고의 `LightsailDefaultKeyPair/key` |

### 서버 → GitHub

| 서버 · 계정 | 옛 키 (GitHub 제목) | 새 배포 키 |
|---|---|---|
| api-dev · api-prod `ai-agent` · `ubuntu` | `github_back` · `github_gateway` (david-mac-ai-agent) | tuk-api-server · tuk-gateway `deploy` |
| api-prod `ai-agent` | `github_admin` (tukapp-ai-agent-admin) | tuk-admin `deploy` |
| api-prod `ai-agent` | `github_push` → tuk-crm-server (tukapp-ai-agent-push) | tuk-crm-server `deploy` |
| api-prod `ubuntu` cron-scripts | 개인 계정 `main` 키 | 걷음 (결정 6) |
| db-prod `ubuntu` cron-scripts | `github_cron_scripts` — GitHub 이 거부, 9/13 이후 pull 안 됨 | 걷음 (결정 6) |
| api-prod `ai-agent` | `github_scheduler` (tukapp-ai-scheduler) — tuk-scheduler 는 prod 에서 돌지 않는다 | 필요 없음. 새 키도 걷는다 |
| nemo · nemo-mac (AWS 밖) | nemo-deploy · nemo-mac · nemo-server | 이 작업 밖 (결정 7) |

서버의 원격 이름 `back` · `gateway` · `infra` · `admin` 은 GitHub 이 리다이렉트하는 옛
이름이다. `infra` 는 tuk-crm-server 다. GitHub Actions 로 서버에 SSH 배포하는 경로는 없다.

## 순서

1. **이 맥 접속.** `~/.ssh/config` 의 사람용 Host 를 새 admin 계정으로 바꾸고 `vpn` 경로를
   고친다. `-ai` Host 는 아직 그대로 둔다. ✅ 2026-09-24 — 6개 Host 접속 확인.
2. **cron-scripts 걷기.** ✅ 2026-09-24. api-prod · db-prod `ubuntu` crontab 의 cronctl 블록을
   빼고(api-prod 의 pm2 소켓 `@reboot` 줄은 유지), 리포와 그것만 쓰던 GitHub 키 · config 를
   걷었다. api-prod 의 개인 계정 키 `~/.ssh/github` 가 여기서 빠졌다. 걷기 전 상태는
   `~/.secrets/archive/servers/20260924-cron-scripts/` 에 있다. 전후로 pm2 8개 online ·
   restart 횟수 불변, nginx · redis · postgres active.
3. **서버 리포.** 운영 디렉터리(`/home/ubuntu/workspace/*`, `ai-agent` 소유, pm2 가 여기서
   돈다)는 건드리지 않고, `deploy` 가 새 배포 키로 공용 자리 `/srv/<리포>` 에 새로 clone 한다.
   키는 `/home/deploy/.ssh/github/<리포>` (0600), 리포마다 `core.sshCommand` 로 지정한다.
   ✅ 2026-09-24 — dev: tuk-api-server · tuk-gateway (develop), prod: tuk-admin ·
   tuk-api-server · tuk-gateway · tuk-crm-server (main). `git fetch` 와 GitHub `last_used`
   확인. prod 는 전후로 pm2 8개 online · restart 횟수 불변.
   tuk-scheduler 는 서버에 받아 둔 곳이 없어 새 키를 넣지 않았다.
   운영 디렉터리가 계속 옛 키로 받으므로 **옛 배포 키 삭제는 앱 이전(결정 5) 뒤로 미룬다.**
   그다음 `-ai` Host 를 `deploy` · `david-deploy` 로 옮긴다.
4. **`-ai` Host 전환.** ✅ 2026-09-24 — `deploy` · `david-deploy` 로.
5. **걷기.** 옛 키가 쓰이지 않는 것을 확인한 뒤.
   ✅ 2026-09-24 authorized_keys — 서버 5대 `ubuntu` · `ai-agent` 에서 `david.personal` ·
   `david.agent` · infra-installer · `aganga7427` · db-prod 깨진 줄을 뺐다. 그 전에 sshd 로그로
   `tuk/personal` 은 config 전환(10:30) 뒤 0회, `tuk/agent` 는 9/21 뒤 0회임을 확인했다.
   pem · garden 키는 남았다. 원본은 각 파일 옆 `.bak.20260924-migration`. 전후 운영 상태 동일.
   `ai-agent` 로는 이제 garden 만 들어간다 — 앱 이전 전까지 운영 디렉터리 갱신은
   admin 에서 `sudo -u ai-agent` 로 한다.
   ✅ 2026-09-24 tuk-scheduler — prod `ai-agent` 의 `github_scheduler` 키와 config 블록을 걷고
   (archive: `~/.secrets/archive/servers/20260924-tuk-scheduler/`), GitHub 옛 키
   `tukapp-ai-scheduler`(156116069)를 지웠다. 금고의 새 키는 금고 화면에서 걷는다.
   - 서버 `ubuntu` · `ai-agent` authorized_keys 의 `david.personal` · `david.agent` ·
     `infra-installer-nemo-mac` · `aganga7427@gmail.com` 줄, db-prod 의 깨진 terminfo 줄
   - 서버에 남은 GitHub 개인키 파일 (archive 로)
   - GitHub 옛 배포 키 9개
   - 로컬 `~/.ssh/tuk/*` · `github/github_main` (archive 로)
   - GitHub 계정 키 `main` · `for-old-laptop`

## 이 작업 밖

- 앱 실행 계정 이전 (결정 5).
- tuk-tunnel 이 관리하던 `~/.ssh/config` 블록 표시는 레거시다. 1단계에서 표시째 바꾼다.

## 앱 실행 계정 이전 (결정 5)

`ubuntu` 로 돌던 앱을 `deploy` 로 옮긴다. 새 것을 옆에 띄우고, nginx 를 넘기고, 옛 것을 끈다.
포트는 새 것(gateway 4002 · back 4102)을 그대로 쓴다 — 되돌리면 한 번 더 넘겨야 한다.

### 리포 수정 (develop, 2026-09-24)

- tuk-api-server `d2bbbcdd` — `start-scheduler.sh` 가 앱 디렉토리를 스크립트 위치에서 구한다
  (전에는 `/home/ubuntu/workspace/back` 고정이라 어디서 띄워도 옛 코드가 돌았다).
  logrotate · 로그 아카이브 경로를 `/srv` · `deploy` 로.
- tuk-gateway `9ecccb4` — logrotate 경로를 `/srv` · `deploy` 로.
- main 에는 아직 없다. prod 는 서버에서 지금 도는 커밋에 이 커밋만 얹어 띄운다.

### dev ✅ 2026-09-24

1. `deploy` 에 nvm v0.40.1 · node v24.16.0 · pnpm 11.5.2 · pm2 7.0.1 (`ubuntu` 와 같은 버전).
   pnpm 은 `strict-dep-builds false` — pnpm 11 이 esbuild 등의 빌드 스크립트를 막고
   `pnpm run` 마다 실패한다. 실행 파일은 이미 있어 결과는 같다.
2. `/srv/<리포>` 를 지금 도는 커밋 + 위 커밋으로 맞춘 로컬 브랜치 `migrate/deploy-account`.
3. back 은 `pnpm-lock.yaml` 이 gitignore 라 운영 디렉토리의 것을 복사. `.env` 는 복사하며
   `PORT` · `GATEWAY_INTERNAL_URL` · `BACK_URL` · `APPLE_ASC_PRIVATE_KEY_PATH` 만 바꾼다.
   `.p8` 은 `/home/deploy/secrets/` 로 복사. back 은 `.env` 를 override 로 읽어 ecosystem 의
   `PORT: '4101'` 보다 `.env` 가 이긴다.
4. 빌드 결과 `dist/server.mjs` 가 운영 중인 것과 바이트 단위로 같음을 확인.
5. back-cluster · gateway-cluster 를 새 포트로 띄우고, 공개 경로 `/l/…` 로 옛 · 새 응답이
   같고 새 back 까지 닿는 것을 확인.
6. nginx upstream 4001 → 4002, `nginx -t` 후 reload. 외부 `dev.tuk.im` 요청이 새 쪽으로만 간다.
7. 옛 back · gateway `pm2 stop` (지우지 않음). 스케줄러는 겹치지 않게 옛 것을 멈추고 새 것을
   띄움 — 공백 약 13초. dev 에는 원래 pulling-scheduler 가 없어 띄우지 않았다.
8. `pm2 save` · `pm2-deploy` 서비스 등록. 옛 `pm2-ubuntu` 는 부팅 자동 시작만 끔.

실수 한 번: nginx 설정 백업을 `sites-enabled` 안에 만들어 nginx 가 그것까지 읽고 검사 ·
reload 에 실패했다. 이전 설정으로 계속 서비스되어 영향은 없었다. **백업은 `sites-enabled`
밖(`/root/nginx-backup/`)에 둔다.**

되돌리기: nginx upstream 을 4001 로, `ubuntu` pm2 에서 `pm2 start back gateway
queue-consumer-scheduler statistics-scheduler`, `deploy` pm2 는 `pm2 stop all`,
`systemctl enable pm2-ubuntu` · `disable pm2-deploy`.

### prod 에서 더 챙길 것

- 앱이 더 있다: push(tuk-crm-server, 4200) · pulling-scheduler. admin 은 pm2 로 돌지 않는다.
- `pm2-logrotate` 모듈 설정을 `deploy` 의 pm2 에도.
- `/etc/logrotate.d/tuk-{back,gateway,pm2,apps}` · `/etc/cron.d/tuk-applog-archive` 경로와 사용자.
- `/home/ubuntu/secrets/` 의 파일들과 `.env` 의 경로 값.
- nginx `dev_shadow` upstream 이 무엇을 가리키는지.
- 금고 기타 `tuk / apple-api` 의 소비처 기록 — dev 경로가 `/home/deploy/secrets/` 로 바뀌었다.

### prod ✅ 2026-09-24 11:46–11:56 (1–6단계)

리포 스크립트는 고치지 않고 서버 전용 설정 `/home/deploy/pm2/tuk.config.cjs` 로 옮겼다
(지금 돌던 값 그대로). 스크립트 정비는 다른 세션에서 한다.

- 런타임: node 24.14.1 · pnpm 10.33.0 · pm2 6.0.14 (prod 의 `ubuntu` 와 같은 버전).
- `/srv` 는 운영 중이던 커밋 그대로(back `38d80772` · gateway `495c4c1` · crm `c55ec84`).
  back · gateway `dist/server.mjs` 바이트 동일. `.env` 는 back 3줄 · gateway 2줄만 바뀜.
- 11:46 새 back(4102) · gateway(4002) 기동 → 11:50:30 nginx `gateway_backend` 4002 로 reload
  → 11:51:35 옛 back · gateway stop → 11:53–11:56 스케줄러 3개 · push 교체.
- 6단계: `pm2 save` · `pm2-deploy` 등록, `pm2-ubuntu` 부팅 자동 시작 끔.
- dev 로의 postback 미러(`dev_shadow`)는 사용자 결정으로 되살리지 않는다. nginx error.log 에
  connection refused 가 계속 쌓인다 — 미러를 쓸 때 다시 설정한다.

pm2 로 옮길 때 드러난 것 (스크립트 정비 때 반영):
- pm2 6 은 `.ts` 스크립트를 bun 으로 돌리려 한다 → `interpreter: "node"`.
- push entrypoint 는 `argv[1]` 로 자신이 메인인지 본다. pm2 가 자기 실행기로 불러오면 조용히
  끝난다(코드 0) → node 를 직접 exec 하는 `/home/deploy/pm2/push.sh` 로 띄운다.

측정 (이 맥에서 1초 간격 `/health` · `/l/…`, 11:47:22–11:56:40, 439회):
- nginx 전환 · 옛 back/gateway 정지 구간: 실패 0, nginx 5xx 0.
- 스케줄러 · push 교체 구간: 감시 timeout 3회. 2회는 nginx 까지 오지 않았고 1회는 서버에서
  약 8초 걸렸다. 2 vCPU 서버에서 tsx 로 뜨는 프로세스의 기동 부하로 보이나 확인하지
  못했다(sysstat 수집이 꺼져 있음).
- 실사용자 요청 1건 실패: 11:54:05 `PUT /api/v1/push/devices` 502 — push 첫 시도가 실패해
  되돌리는 사이 push 가 비어 있었다.

되돌리기: nginx 백업 `/root/nginx-backup/tuk.20260924-migration`. `ubuntu` pm2 의 앱 8개는
stopped 로 남아 있다.

### prod 7단계 — 로그 ✅ 2026-09-24

- logrotate: `/etc/logrotate.d/tuk-{pm2,back,gateway,apps}` 를 새 경로로 교체
  (`/home/deploy/.pm2/logs` 14일 · `su deploy deploy`, `/srv/{tuk-api-server,tuk-gateway,tuk-crm-server}/logs`
  30일 · `su deploy workspace`). 원본 `/root/logrotate-backup/`.
- 옛 경로 5곳(pm2 · back · gateway · push · admin)의 압축 안 된 로그 141개를 gzip 하고, `.gz` 655개를
  S3 `applog-archive/tukapp/{pm2,back,gateway,push,admin}/` 에 올렸다(새로 207 · 이미 있음 448 · 실패 0).
  S3 목록과 로컬 이름 · 크기를 대조해 빠진 것 0. 로컬 원본은 아직 지우지 않았다.
  push · admin 로그는 이번에 처음 올라갔다(전에는 아카이브 대상이 아니었다).
- 매일 아카이브: `/usr/local/bin/tuk-applog-archive-deploy.sh` (root:deploy 750), 태그
  `deploy-{pm2,back,gateway,push}` — 옛 스케줄러 로그와 이름이 같아 같은 태그면 S3 에서 덮어쓴다.
  `/etc/cron.d/tuk-applog-archive` 는 03:30 `deploy` 로 교체. 원본 `/root/cron-backup/`.
- 스크립트 정비 때 맞출 것: 리포 `ops/tuk-applog-archive.sh`(develop)의 태그를 `deploy-*` 로,
  `/home/ubuntu/logs.sh`(로그 보기)가 옛 경로를 본다.

### deploy 외 정리 (2026-09-24, 사용자 승인 — 배포 후 수 시간 정상)

- 보관: 옛 운영 디렉토리(`/home/ubuntu/workspace/*` · `/home/ubuntu/app/*`, node_modules · dist · .next ·
  logs 제외, `.git` 포함 — prod back · gateway 에 stash 있음), `ubuntu` · `ai-agent` 의 GitHub 키 · ssh
  config, `/home/ubuntu/secrets` → `~/.secrets/archive/servers/20260924-app-migration/<host>/` (553MB).
  prod admin 의 `.data/users.json`, back 의 git 밖 마이그레이션 스크립트 포함 확인.
- 프로세스: `ubuntu` pm2 `delete all` · `save --force` · `kill`, `pm2-ubuntu` 유닛 삭제, `ai-agent` pm2 `kill`,
  `ai-agent` `.bashrc` 의 `PM2_HOME=/home/ubuntu/.pm2` 줄 제거(원본 `.bashrc.bak.20260924`),
  dev 의 `pm2 logs back` · prod 의 `logs.sh` tail · tmux `tuk-logs` 종료. 두 서버의 pm2 데몬은 deploy 하나.
- GitHub 옛 배포 키 4개 삭제 (서버 키 파일과 지문 일치 확인): tuk-api-server · tuk-gateway
  `david-mac-ai-agent`, tuk-admin `tukapp-ai-agent-admin`, tuk-crm-server `tukapp-ai-agent-push`.
- 서버 파일 삭제(옛 디렉토리 · secrets · GitHub 키 파일 · authorized_keys 백업)는 권한 분류기가 막아
  사용자가 실행한다.
