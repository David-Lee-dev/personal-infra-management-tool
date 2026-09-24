# 슬라이스 2 — AWS IAM

## 규칙

모든 AWS 권한은 **최소 권한 IAM 사용자 + 액세스 키**로 준다. 금고가 발급하고,
필요한 곳의 `.env` 에는 사람이 넣는다. 인스턴스에 역할을 붙이지 않는다 — 역할은
인스턴스 안에 숨어 금고의 기록 밖에 있다. 모든 자격은 금고에서 나가고, 어디에
넣었는지는 금고에 적는다.

1. **IAM 하나에 권한 하나.** 서비스 하나, 범위 하나. 인라인 정책 하나에 리소스를
   특정한다. `*` 리소스와 관리형 정책은 쓰지 않는다 — 떨어진 정책이 생긴다.
2. **환경마다 따로.** prod 와 dev 가 같은 IAM 을 쓰지 않는다.
3. **이름은 `<앱>-<환경>-<권한>-iam-<YYYYMMDD>`.** 이름만 보고 누가 어디서 무엇을
   하는지, 언제 만든 세대인지 안다. 권한 조각은 정책의 서비스다. 같은 앱 · 환경에
   그 조각을 쓰는 IAM 이 있으면 대상을 견준다 — 같으면 새 세대(조각 그대로, 날짜만
   다름), 다르면 대상의 마지막 경로 조각을 붙인다(`s3-applog-archive`). 같은 날 같은
   이름이 또 나오면 `-2` 부터 붙인다.
4. **`.env` 변수에 권한을 넣는다.** `AWS_<권한>_ACCESS_KEY_ID` ·
   `AWS_<권한>_SECRET_ACCESS_KEY`. `.env` 하나에 IAM 이 여럿 들어간다.
5. **상태는 있거나 없거나.** 중간 상태를 화면에 두지 않는다.
6. **소비처는 기록만 한다.** `.env` 에 넣고 빼는 일은 사람이 한다. 금고는 붙여 넣을
   두 줄을 복사해 줄 뿐 파일을 건드리지 않는다. 접근을 끊는 수단은 파일이 아니라
   IAM 쪽 키다 — 키가 없으면 `.env` 에 값이 남아 있어도 쓸 수 없다.
7. **키를 바꾸는 기능은 없다.** 새 IAM 을 만들고 옛 것은 사람이 치운다. 자동 교체는
   기록에서 빠진 소비처를 끊는다.
8. **삭제는 키가 30일 넘게 쓰이지 않았을 때만.** 기준은 AWS 가 기록한 마지막 사용
   시각이고, 한 번도 쓰이지 않았으면 발급 시각이다. 경계는 31일째부터다 — AWS 의
   시각은 UTC 날짜라 하루 어긋날 수 있다. 시각을 읽지 못하면 지우지 않는다.
   소비처 기록이 틀려도 아직 쓰이는 키는 지워지지 않는다.
   **삭제 가능일**을 기록에 둔다. 마지막 사용은 앞으로만 움직이므로 이 날짜는 하한이다 —
   만들 때 발급일 + 31일로 정하고, AWS 에 물을 때마다(조회 · 거부된 삭제) 다시 적는다.
   그 전에는 묻지 않고 삭제를 잠근다.
9. **규칙에 맞지 않는 기존 것은 새로 만들어 옮긴 뒤 폐기한다.** 고쳐 맞추지 않는다.

예외: AWS 서비스(Scheduler · Lambda 등)가 맡는 역할. 키를 받을 수 없어 역할만 된다.
이 도구의 관리 대상이 아니다. 2026-09-23 현재 계정에 해당하는 것은 없다.

마스터 자격(`david-lee-admin`)은 이 규칙 밖이다. 계정 관리 탭에서 다룬다.

GitHub 배포 키도 같은 까닭으로 **GitHub 쪽 제목** 끝에 등록한 날을 붙인다
(`secrets/develop-20260924`). 로컬 경로에는 붙이지 않는다 — 리포의 `core.sshCommand`
가 그 경로를 가리킨다. 인스턴스 계정은 리눅스 로그인 이름이라 붙이지 않고, pem 은
이름을 AWS 가 정한다.

## 옮기기

| 지금 | 새 이름 |
|---|---|
| `tuk-api-server-s3-handler` | `tuk-api-prod-s3-iam-<날짜>` · `tuk-api-dev-s3-iam-<날짜>` |
| `tuk-bedrock` | `tuk-api-prod-bedrock-iam-<날짜>` · `tuk-api-dev-bedrock-iam-<날짜>` |
| `market-analysis-bedrock` | `market-analysis-prod-bedrock-iam-<날짜>` · `market-analysis-local-bedrock-iam-<날짜>` |
| `tuk-api-server-role` (역할) | 권한을 위 IAM 들로 옮긴다 |
| `tukdatabase-prod-role` (역할) | `tuk-db-prod-s3-iam-<날짜>` |
| `tukdatabase-dev-role` (역할) | `tuk-db-dev-s3-iam-<날짜>` |

DB 서버는 **마지막에** 옮긴다. pgbackrest 가 역할로 S3 에 백업을 올리고 있어,
틀리면 백업이 조용히 멈춘다.

## 시스템이 완성되면 정리할 것

새 IAM 이 소비처에 들어가 동작이 확인된 뒤에 한다. 옛 IAM 사용자는 규칙 8 과 같은
기준 — 옛 키가 30일 넘게 쓰이지 않음 — 을 확인하고 지운다. 지우기 전에 정의를
`~/.secrets/archive/aws/<날짜>/` 에 보관하고 근거를 `TOMBSTONE.md` 에 남긴다.

### AWS

- [ ] IAM 사용자 폐기 — `tuk-api-server-s3-handler` · `tuk-bedrock` · `market-analysis-bedrock`
- [ ] 인스턴스 역할 떼고 폐기 — `tuk-api-server-role` · `tukdatabase-prod-role` · `tukdatabase-dev-role` (DB 는 마지막)
- [ ] `dev-tuk-api-server-scheduler-role` 삭제 — 2026-06-08 생성 후 한 번도 쓰이지 않음. 이를 맡는 Scheduler 일정 없음
- [ ] 떨어진 관리형 정책 `dev-tuk-api-server-power-only` 삭제 — 2026-09-23 `tuk-dev-power` 삭제로 연결 주체 없음

### 옛 키가 남은 파일

- [ ] `~/Documents/tuk-api-server-s3-handler_accessKeys.csv` — 시크릿 평문, 권한 644, 2026-04-29 부터
- [x] tukapp-prod `~/workspace/back/.env.bak` · `.env.bak.202609091949` · `.env.bak.202607061637` — 2026-09-24 옛 운영 디렉토리째 보관 후 삭제
- [x] tukapp-dev `~/workspace/back/.env.bak.202609091949` · `.env.bak-0821` — 2026-09-24 같음
- [ ] tukapp-prod · tukapp-dev `/srv/*/.env` — 옛 키가 그대로 복사돼 있다. 새 IAM 으로 바뀌는지 확인
- [ ] 이 맥 `11_tuk/02_tuk-api-server/.env` · `.env.dev` · `.env.local` · `.env.prod`, `11_tuk/00_tuk-admin/.env.local` — 새 키로 바뀌는지 확인
- [ ] nemo `/srv/infra/workspace/11_tuk/00_tuk-admin/.env.local` — 새 키로 바뀌는지 확인

### 서버 · GitHub · 키체인

- [x] gonggugyeong `ubuntu` 의 `authorized_keys` 에서 `tuk/personal` 줄 제거 — 2026-09-24 서버 5대 전부 (docs/04)
- [ ] 옛 GitHub 배포 키 9개 삭제 — EC2 전환 후. 2026-09-24 tuk 5개 삭제(back · gateway · admin · push · scheduler), nemo 쪽 4개 남음(로컬 서버 단계)
- [ ] 키체인 `github.com / David-Lee-dev` 항목 정리

### 확인이 필요한 것

- [ ] `garden-kim` — 콘솔 사용자, 관리형 정책 3개(EC2 · IAM · S3). 사람 계정이라 규칙 대상인지 정해야 한다
