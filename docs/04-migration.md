# 기존 연결 정보를 시크릿 저장소 발급분으로 교체

2026-09-24 스캔. 서버 5대 · GitHub 리포 20개 · 로컬 `~/.ssh`를 읽기만 해서 수집했다.
새 인스턴스 계정 5개(admin · david-admin)로 모든 서버에 접속되는 것도 이때 확인했다.

## 원칙

- 새 것을 연결하고, 동작을 확인한 다음 기존 것을 제거한다. 한 번에 사용 위치 하나씩 진행한다.
- `ubuntu`의 pem 키는 비상용으로 끝까지 유지한다.
- `~/.ssh/config`는 사람이 수정한다. 도구는 바꿀 내용을 diff로 보여 준다.
- 제거한 개인키는 삭제하지 않고 시크릿 저장소 `archive/`로 옮긴다.

## 결정 (2026-09-24, 사용자)

1. garden의 접근(`garden.personal` · `garden.agent`)은 유지한다. 따라서 `ai-agent`
   계정은 남기고, 그 안의 `david.agent` · infra-installer 줄만 제거한다.
2. `infra-installer-nemo-mac`은 시크릿 저장소의 초기 버전이었다. 모든 서버에서 제거한다.
3. AI 에이전트는 `deploy` 계정을 함께 사용한다. `-ai` Host는 `deploy`에 리포가 준비된 뒤 전환한다.
4. db-prod `ubuntu`의 `aganga7427@gmail.com` 키는 출처를 알 수 없다. 제거한다.
5. 앱 실행 계정 이전은 이 작업 범위 밖이다. 계정 정보 전환이 끝난 뒤 별도로 진행한다 —
   `deploy`로 tuk · gonggugyeong을 추가로 실행하고, 그다음 `ubuntu`로 실행 중이던 프로세스를 중지한다.
6. cron-scripts는 인스턴스마다 두지 않고 중앙에서 확인하는 방식으로 바꾼다. 인스턴스에서는
   중지하고 제거한다.
7. nemo · nemo-mac 같은 로컬 서버는 이 작업에서 제외한다. AWS 정리가 끝난 뒤 별도로 진행한다.
   따라서 로컬 `~/.ssh/tuk/*` · `github_main` · GitHub 계정 키도 그때 제거한다.

## 기존 연결 → 새 연결

### 로컬 → 서버

| Host | 현재 | 변경 후 |
|---|---|---|
| `tukapp-dev` · `tukdb-dev` · `tukapp-prod` · `tukdb-prod` | `ubuntu` + `tuk/personal` | `admin` + 시크릿 저장소 키 |
| `gonggugyeong-prod` | `ubuntu` + `tuk/personal` | `david-admin` + 시크릿 저장소 키 |
| `tukapp-dev-ai` · `tukapp-prod-ai` · `gonggugyeong-prod-ai` | `ai-agent` + `tuk/agent` | `deploy` · `david-deploy` (3단계 뒤) |
| `vpn` | `~/.ssh/lightsail-seoul.pem` (시크릿 저장소로 이동해 없음) | 시크릿 저장소의 `LightsailDefaultKeyPair/key` |

### 서버 → GitHub

| 서버 · 계정 | 기존 키 (GitHub 제목) | 새 배포 키 |
|---|---|---|
| api-dev · api-prod `ai-agent` · `ubuntu` | `github_back` · `github_gateway` (david-mac-ai-agent) | tuk-api-server · tuk-gateway `deploy` |
| api-prod `ai-agent` | `github_admin` (tukapp-ai-agent-admin) | tuk-admin `deploy` |
| api-prod `ai-agent` | `github_push` → tuk-crm-server (tukapp-ai-agent-push) | tuk-crm-server `deploy` |
| api-prod `ubuntu` cron-scripts | 개인 계정 `main` 키 | 제거 (결정 6) |
| db-prod `ubuntu` cron-scripts | `github_cron_scripts` — GitHub이 거부, 9/13 이후 pull 불가 | 제거 (결정 6) |
| api-prod `ai-agent` | `github_scheduler` (tukapp-ai-scheduler) — tuk-scheduler는 prod에서 실행되지 않는다 | 필요 없음. 새 키도 제거한다 |
| nemo · nemo-mac (AWS 외부) | nemo-deploy · nemo-mac · nemo-server | 이 작업 범위 밖 (결정 7) |

서버의 원격 이름 `back` · `gateway` · `infra` · `admin`은 GitHub이 리다이렉트하는 예전
이름이다. `infra`는 tuk-crm-server다. GitHub Actions로 서버에 SSH 배포하는 경로는 없다.

## 순서

1. **로컬 접속.** `~/.ssh/config`의 사람용 Host를 새 admin 계정으로 바꾸고 `vpn` 경로를
   수정한다. `-ai` Host는 아직 그대로 둔다. ✅ 2026-09-24 — 6개 Host 접속 확인.
2. **cron-scripts 제거.** ✅ 2026-09-24. api-prod · db-prod `ubuntu` crontab의 cronctl 블록을
   삭제하고(api-prod의 pm2 소켓 `@reboot` 줄은 유지), 리포와 그 리포만 사용하던 GitHub 키 · config를
   제거했다. api-prod의 개인 계정 키 `~/.ssh/github`가 이 단계에서 제거되었다. 제거 전 상태는
   `~/.secrets/archive/servers/20260924-cron-scripts/`에 있다. 작업 전후 pm2 8개 online ·
   restart 횟수 변동 없음, nginx · redis · postgres active.
3. **서버 리포.** 운영 디렉터리(`/home/ubuntu/workspace/*`, `ai-agent` 소유, pm2가 여기서
   실행된다)는 건드리지 않고, `deploy`가 새 배포 키로 공용 위치 `/srv/<리포>`에 새로 clone한다.
   키는 `/home/deploy/.ssh/github/<리포>` (0600), 리포마다 `core.sshCommand`로 지정한다.
   ✅ 2026-09-24 — dev: tuk-api-server · tuk-gateway (develop), prod: tuk-admin ·
   tuk-api-server · tuk-gateway · tuk-crm-server (main). `git fetch`와 GitHub `last_used`
   확인. prod는 작업 전후 pm2 8개 online · restart 횟수 변동 없음.
   tuk-scheduler는 서버에 clone해 둔 곳이 없어 새 키를 배치하지 않았다.
   운영 디렉터리가 계속 기존 키로 pull하므로 **기존 배포 키 삭제는 앱 이전(결정 5) 이후로 미룬다.**
   그다음 `-ai` Host를 `deploy` · `david-deploy`로 전환한다.
4. **`-ai` Host 전환.** ✅ 2026-09-24 — `deploy` · `david-deploy`로 전환.
5. **제거.** 기존 키가 쓰이지 않는 것을 확인한 뒤 진행한다.
   ✅ 2026-09-24 authorized_keys — 서버 5대 `ubuntu` · `ai-agent`에서 `david.personal` ·
   `david.agent` · infra-installer · `aganga7427` · db-prod의 깨진 줄을 삭제했다. 그 전에 sshd 로그로
   `tuk/personal`은 config 전환(10:30) 이후 0회, `tuk/agent`는 9/21 이후 0회 사용되었음을 확인했다.
   pem · garden 키는 유지했다. 원본은 각 파일 옆 `.bak.20260924-migration`. 작업 전후 운영 상태 동일.
   이제 `ai-agent`로는 garden만 접속한다 — 앱 이전 전까지 운영 디렉터리 갱신은
   admin에서 `sudo -u ai-agent`로 한다.
   ✅ 2026-09-24 tuk-scheduler — prod `ai-agent`의 `github_scheduler` 키와 config 블록을 제거하고
   (archive: `~/.secrets/archive/servers/20260924-tuk-scheduler/`), GitHub의 기존 키
   `tukapp-ai-scheduler`(156116069)를 삭제했다. 시크릿 저장소의 새 키는 시크릿 저장소 화면에서 제거한다.
   - 서버 `ubuntu` · `ai-agent` authorized_keys의 `david.personal` · `david.agent` ·
     `infra-installer-nemo-mac` · `aganga7427@gmail.com` 줄, db-prod의 깨진 terminfo 줄
   - 서버에 남은 GitHub 개인키 파일 (archive로)
   - GitHub 기존 배포 키 9개
   - 로컬 `~/.ssh/tuk/*` · `github/github_main` (archive로)
   - GitHub 계정 키 `main` · `for-old-laptop`

## 이 작업의 범위 밖

- 앱 실행 계정 이전 (결정 5).
- tuk-tunnel이 관리하던 `~/.ssh/config` 블록 표시는 레거시다. 1단계에서 표시까지 함께 바꾼다.

## 앱 실행 계정 이전 (결정 5)

`ubuntu`로 실행하던 앱을 `deploy`로 이전한다. 새 프로세스를 나란히 실행하고, nginx 트래픽을 전환한 뒤 기존 프로세스를 중지한다.
포트는 새 포트(gateway 4002 · back 4102)를 그대로 쓴다 — 롤백하면 한 번 더 전환해야 한다.

### 리포 수정 (develop, 2026-09-24)

- tuk-api-server `d2bbbcdd` — `start-scheduler.sh`가 앱 디렉토리를 스크립트 위치에서 찾는다
  (전에는 `/home/ubuntu/workspace/back`으로 하드코딩되어 있어 어디서 실행해도 기존 코드가 실행되었다).
  logrotate · 로그 아카이브 경로를 `/srv` · `deploy`로 변경.
- tuk-gateway `9ecccb4` — logrotate 경로를 `/srv` · `deploy`로 변경.
- main에는 아직 반영되지 않았다. prod는 서버에서 현재 실행 중인 커밋에 이 커밋만 추가해 실행한다.

### dev ✅ 2026-09-24

1. `deploy`에 nvm v0.40.1 · node v24.16.0 · pnpm 11.5.2 · pm2 7.0.1 설치(`ubuntu`와 같은 버전).
   pnpm은 `strict-dep-builds false` — pnpm 11이 esbuild 등의 빌드 스크립트를 차단해
   `pnpm run`마다 실패한다. 실행 파일은 이미 있어 결과는 같다.
2. `/srv/<리포>`를 현재 실행 중인 커밋 + 위 커밋으로 맞춘 로컬 브랜치 `migrate/deploy-account`.
3. back은 `pnpm-lock.yaml`이 gitignore 대상이라 운영 디렉토리의 파일을 복사. `.env`는 복사하면서
   `PORT` · `GATEWAY_INTERNAL_URL` · `BACK_URL` · `APPLE_ASC_PRIVATE_KEY_PATH`만 바꾼다.
   `.p8`은 `/home/deploy/secrets/`로 복사. back은 `.env`를 override로 읽으므로 ecosystem의
   `PORT: '4101'`보다 `.env`가 우선한다.
4. 빌드 결과 `dist/server.mjs`가 운영 중인 것과 바이트 단위로 같음을 확인.
5. back-cluster · gateway-cluster를 새 포트로 실행하고, 공개 경로 `/l/…`로 기존 · 새 응답이
   같고 새 back까지 요청이 도달하는 것을 확인.
6. nginx upstream 4001 → 4002, `nginx -t` 후 reload. 외부 `dev.tuk.im` 요청이 새 쪽으로만 간다.
7. 기존 back · gateway `pm2 stop` (삭제하지 않음). 스케줄러는 겹치지 않게 기존 것을 중지하고 새 것을
   시작 — 공백 약 13초. dev에는 원래 pulling-scheduler가 없어 실행하지 않았다.
8. `pm2 save` · `pm2-deploy` 서비스 등록. 기존 `pm2-ubuntu`는 부팅 시 자동 시작만 해제.

실수 1건: nginx 설정 백업을 `sites-enabled` 안에 만들어 nginx가 그 파일까지 읽는 바람에 검사 ·
reload에 실패했다. 이전 설정으로 계속 서비스되어 영향은 없었다. **백업은 `sites-enabled`
밖(`/root/nginx-backup/`)에 둔다.**

롤백: nginx upstream을 4001로, `ubuntu` pm2에서 `pm2 start back gateway
queue-consumer-scheduler statistics-scheduler`, `deploy` pm2는 `pm2 stop all`,
`systemctl enable pm2-ubuntu` · `disable pm2-deploy`.

### prod에서 추가로 확인할 것

- 앱이 더 있다: push(tuk-crm-server, 4200) · pulling-scheduler. admin은 pm2로 실행되지 않는다.
- `pm2-logrotate` 모듈 설정을 `deploy`의 pm2에도 적용.
- `/etc/logrotate.d/tuk-{back,gateway,pm2,apps}` · `/etc/cron.d/tuk-applog-archive`의 경로와 사용자.
- `/home/ubuntu/secrets/`의 파일들과 `.env`의 경로 값.
- nginx `dev_shadow` upstream이 무엇을 가리키는지.
- 시크릿 저장소 기타 항목 `tuk / apple-api`의 사용 위치 기록 — dev 경로가 `/home/deploy/secrets/`로 바뀌었다.

### prod ✅ 2026-09-24 11:46–11:56 (1–6단계)

리포 스크립트는 수정하지 않고 서버 전용 설정 `/home/deploy/pm2/tuk.config.cjs`로 옮겼다
(기존 실행 값 그대로). 스크립트 정비는 별도 세션에서 한다.

- 런타임: node 24.14.1 · pnpm 10.33.0 · pm2 6.0.14 (prod의 `ubuntu`와 같은 버전).
- `/srv`는 운영 중이던 커밋 그대로(back `38d80772` · gateway `495c4c1` · crm `c55ec84`).
  back · gateway `dist/server.mjs` 바이트 동일. `.env`는 back 3줄 · gateway 2줄만 변경.
- 11:46 새 back(4102) · gateway(4002) 시작 → 11:50:30 nginx `gateway_backend`를 4002로 변경해 reload
  → 11:51:35 기존 back · gateway stop → 11:53–11:56 스케줄러 3개 · push 교체.
- 6단계: `pm2 save` · `pm2-deploy` 등록, `pm2-ubuntu` 부팅 시 자동 시작 해제.
- dev로의 postback 미러(`dev_shadow`)는 사용자 결정에 따라 복구하지 않는다. nginx error.log에
  connection refused가 계속 쌓인다 — 미러가 필요할 때 다시 설정한다.

pm2로 이전하며 발견한 문제 (스크립트 정비 때 반영):
- pm2 6은 `.ts` 스크립트를 bun으로 실행하려 한다 → `interpreter: "node"`.
- push entrypoint는 `argv[1]`로 자신이 메인 모듈인지 판단한다. pm2가 자체 실행기로 불러오면 오류 없이
  종료된다(코드 0) → node를 직접 exec하는 `/home/deploy/pm2/push.sh`로 실행한다.

측정 (로컬에서 1초 간격 `/health` · `/l/…`, 11:47:22–11:56:40, 439회):
- nginx 전환 · 기존 back/gateway 중지 구간: 실패 0, nginx 5xx 0.
- 스케줄러 · push 교체 구간: 모니터링 timeout 3회. 2회는 nginx까지 도달하지 않았고 1회는 서버에서
  약 8초 걸렸다. 2 vCPU 서버에서 tsx로 실행되는 프로세스의 기동 부하로 보이나 확인하지
  못했다(sysstat 수집이 꺼져 있음).
- 실사용자 요청 1건 실패: 11:54:05 `PUT /api/v1/push/devices` 502 — push 첫 시도가 실패해
  롤백하는 동안 push가 비어 있었다.

롤백: nginx 백업 `/root/nginx-backup/tuk.20260924-migration`. `ubuntu` pm2의 앱 8개는
stopped 상태로 남아 있다.

### prod 7단계 — 로그 ✅ 2026-09-24

- logrotate: `/etc/logrotate.d/tuk-{pm2,back,gateway,apps}`를 새 경로로 교체
  (`/home/deploy/.pm2/logs` 14일 · `su deploy deploy`, `/srv/{tuk-api-server,tuk-gateway,tuk-crm-server}/logs`
  30일 · `su deploy workspace`). 원본 `/root/logrotate-backup/`.
- 기존 경로 5곳(pm2 · back · gateway · push · admin)의 압축되지 않은 로그 141개를 gzip으로 압축하고, `.gz` 655개를
  S3 `applog-archive/tukapp/{pm2,back,gateway,push,admin}/`에 업로드했다(신규 207 · 기존 448 · 실패 0).
  S3 목록과 로컬 이름 · 크기를 대조해 누락 0. 로컬 원본은 아직 삭제하지 않았다.
  push · admin 로그는 이번에 처음 업로드되었다(전에는 아카이브 대상이 아니었다).
- 매일 아카이브: `/usr/local/bin/tuk-applog-archive-deploy.sh` (root:deploy 750), 태그
  `deploy-{pm2,back,gateway,push}` — 기존 스케줄러 로그와 이름이 같아, 태그가 같으면 S3에서 덮어쓴다.
  `/etc/cron.d/tuk-applog-archive`는 03:30 `deploy` 실행으로 교체. 원본 `/root/cron-backup/`.
- 스크립트 정비 때 맞출 것: 리포 `ops/tuk-applog-archive.sh`(develop)의 태그를 `deploy-*`로 변경,
  `/home/ubuntu/logs.sh`(로그 보기)가 기존 경로를 참조한다.

### deploy 외 정리 (2026-09-24, 사용자 승인 — 배포 후 수 시간 정상)

- 보관: 기존 운영 디렉토리(`/home/ubuntu/workspace/*` · `/home/ubuntu/app/*`, node_modules · dist · .next ·
  logs 제외, `.git` 포함 — prod back · gateway에 stash 있음), `ubuntu` · `ai-agent`의 GitHub 키 · ssh
  config, `/home/ubuntu/secrets` → `~/.secrets/archive/servers/20260924-app-migration/<host>/` (553MB).
  prod admin의 `.data/users.json`, back의 git에 포함되지 않은 마이그레이션 스크립트 포함 확인.
- 프로세스: `ubuntu` pm2 `delete all` · `save --force` · `kill`, `pm2-ubuntu` 유닛 삭제, `ai-agent` pm2 `kill`,
  `ai-agent` `.bashrc`의 `PM2_HOME=/home/ubuntu/.pm2` 줄 제거(원본 `.bashrc.bak.20260924`),
  dev의 `pm2 logs back` · prod의 `logs.sh` tail · tmux `tuk-logs` 종료. 두 서버의 pm2 데몬은 deploy 하나만 남았다.
- GitHub 기존 배포 키 4개 삭제 (서버 키 파일과 지문 일치 확인): tuk-api-server · tuk-gateway
  `david-mac-ai-agent`, tuk-admin `tukapp-ai-agent-admin`, tuk-crm-server `tukapp-ai-agent-push`.
- 서버 파일 삭제(기존 디렉토리 · secrets · GitHub 키 파일 · authorized_keys 백업)는 권한 분류기가 차단해
  사용자가 직접 실행한다.
