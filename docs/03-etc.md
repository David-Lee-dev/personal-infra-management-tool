# 슬라이스 3 — 기타

## 보관 대상

**다시 받을 수 없는 파일**만 보관한다. 발급할 때 한 번만 내려받을 수 있거나, 분실하면
복구 부담이 큰 파일이다.

| 보관한다 | 이유 |
|---|---|
| Apple API 키 `.p8` | 발급할 때 한 번만 내려받을 수 있다 |
| Android 업로드 키스토어 `.jks` | 분실하면 Play Console에 업로드 키 재설정을 요청해야 한다 |
| Firebase Admin · Google Play 서비스 계정 JSON | 생성할 때 한 번만 내려받을 수 있다 |

| 보관하지 않는다 | 이유 |
|---|---|
| `.env`의 API 키 | 각 서비스 콘솔에서 다시 확인할 수 있다 |
| Firebase 설정 (`google-services.json` · `GoogleService-Info.plist`) | 비밀값이 아니다. 앱에 포함되어 배포된다 |
| 콘솔 비밀번호 | 재설정할 수 있다 |

## 규칙

1. **항목 하나 = 파일 하나 + 그 파일을 여는 값.** 키스토어는 비밀번호 없이는 쓸 수
   없으므로 `storePassword` · `keyPassword` · `keyAlias`를 한 묶음으로 가져온다. 값만
   있는 항목은 없다.
2. **위치는 `<프로젝트> / <이름>`.** 예: `tuk-app / android-upload`.
3. **생성하지 않고 가져오기만 한다.** 이 도구가 발급할 수 없는 것들이다.
4. **파일은 이동한다.** 사본을 남기면 파일이 여러 곳에 흩어진 상태가 그대로 유지된다. 가져오기
   전에 해시로 같은 파일을 찾아 보여 준다.
   화면: 자격 증명 › 기타 › [＋ 가져오기] (2026-09-28). 금고의 사본이 원본과 해시가 같을 때만 원본을 지운다.
   해시로 흩어진 사본을 찾아 보여 주는 일은 아직 없다.
5. **삭제 기능은 없다.** 제거하면 보관소로 옮길 뿐이다. 다시 받을 수 없는 파일이기 때문이다.
6. **사용 위치는 기록만 한다.** 빌드 설정(`key.properties`의 `storeFile` 등)은 사람이
   수정한다. 도구는 바꿀 한 줄을 보여 준다.

## 시크릿 저장소 내 위치

```
keys/etc/<프로젝트>/<이름>/
  item.toml     기록 — 종류 · 용도 · 해시 · 소비처
  files/        들인 파일 그대로  0600
  values.env    그 파일을 여는 값  0600
```

## 가져온 항목 (2026-09-23)

| 항목 | 파일 | 값 |
|---|---|---|
| `tuk-app / android-upload` | `upload-keystore.jks` | `storePassword` · `keyPassword` · `keyAlias` |
| `gong-gugyeong / android-upload` | `upload-keystore.jks` | `storePassword` · `keyPassword` · `keyAlias` |
| `tuk / apple-api` | `AuthKey_XZRTQ6ZPQ8.p8` | — |

파일은 해시를 확인하며 옮겼고, 값은 `key.properties`의 세 줄을 시크릿 저장소로 복사했다.
`key.properties`는 빌드가 읽는 파일이라 제자리에 둔다.

이동 후 수정해야 하는 설정:

| 파일 | 줄 |
|---|---|
| `11_tuk/03_tuk-app/android/key.properties` | `storeFile=/Users/david/.secrets/keys/etc/tuk-app/android-upload/files/upload-keystore.jks` |
| `61_gong-gugyeong/app/android/key.properties` | `storeFile=/Users/david/.secrets/keys/etc/gong-gugyeong/android-upload/files/upload-keystore.jks` |
| `11_tuk/02_tuk-api-server/.env.local` | `APPLE_ASC_PRIVATE_KEY_PATH="/Users/david/.secrets/keys/etc/tuk/apple-api/files/AuthKey_XZRTQ6ZPQ8.p8"` |

`.p8`은 tukapp-prod · tukapp-dev에도 사본이 있다. 서버가 사용하는 파일이라 그대로 두고, 사용 위치로
기록한다. 2026-09-24 앱 실행 계정을 이전하면서 서버의 파일 위치가 `/home/deploy/secrets/`로 바뀌었다.

## 구현

- 1단계 ✅ 2026-09-24 — 목록 · 상세 화면에 시크릿 저장소의 실제 기록을 표시한다. 용도, 사용 위치 기록(추가 · 제거),
  여는 값 복사, Android `key.properties` 줄 복사. 코드: core `etc`, local `etc.rs`, gui `command/etc.rs`.
- 2단계 — 가져오기(해시로 같은 파일 찾기 · 이동)와 보관소로 이동. 화면의 버튼도 그때 추가한다.

## 시스템이 완성되면 정리할 것

- [ ] `~/workspace/upload-keystore.jks` — tuk-app 키스토어와 같은 파일(해시 일치). 따로 남아 있던 사본
- [ ] `~/Downloads/AuthKey_XZRTQ6ZPQ8.p8` — `~/.secrets/tuk/`의 파일과 같은 파일. 내려받은 원본
- [ ] `~/Documents/psql-tunnel.pem` — Naver Cloud 서버용. 서버를 모두 폐기해 쓰지 않음
- [ ] `~/Documents/tuk/david-lee-admin_credentials.csv` — 관리자 IAM 콘솔 비밀번호 평문, 644. 유효 여부 미확인
- [ ] `80_my_projects/.05_market-analysis-backup-20260827/.secrets/` — API 키 `.env` 백업 사본 3개
- [x] `101_android-upload-key/gong-gugyeong/pwd.txt` — 키스토어 비밀번호 평문 사본. 시크릿 저장소 · key.properties와 해시 일치 확인 후 2026-09-23 삭제
