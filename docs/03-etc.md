# 슬라이스 3 — 기타

## 무엇을 담나

**다시 받을 수 없는 파일**만 담는다. 발급할 때 한 번만 내려받을 수 있거나, 잃으면
되돌리기가 무거운 것들이다.

| 담는다 | 까닭 |
|---|---|
| Apple API 키 `.p8` | 발급할 때 한 번만 내려받는다 |
| Android 업로드 키스토어 `.jks` | 잃으면 Play Console 에 업로드 키 재설정을 요청해야 한다 |
| Firebase Admin · Google Play 서비스 계정 JSON | 만들 때 한 번만 내려받는다 |

| 담지 않는다 | 까닭 |
|---|---|
| `.env` 의 API 키 | 각 서비스 콘솔에서 다시 확인할 수 있다 |
| Firebase 설정 (`google-services.json` · `GoogleService-Info.plist`) | 비밀값이 아니다. 앱에 실려 배포된다 |
| 콘솔 비밀번호 | 재설정할 수 있다 |

## 규칙

1. **항목 하나 = 파일 하나 + 그 파일을 여는 값.** 키스토어는 비밀번호 없이는 쓸 수
   없으므로 `storePassword` · `keyPassword` · `keyAlias` 를 한 묶음으로 들인다. 값만
   있는 항목은 없다.
2. **자리는 `<프로젝트> / <이름>`.** 예: `tuk-app / android-upload`.
3. **만들지 않고 들이기만 한다.** 이 도구가 발급할 수 없는 것들이다.
4. **파일은 옮긴다.** 사본을 남기면 흩어진 상태가 그대로다. 들이기 전에 해시로 같은
   파일을 찾아 보여 준다.
5. **지우는 길이 없다.** 걷어내면 보관소로 옮길 뿐이다. 다시 받을 수 없는 것이다.
6. **소비처는 기록만 한다.** 빌드 설정(`key.properties` 의 `storeFile` 등)은 사람이
   고친다. 도구는 바꿀 한 줄을 보여 준다.

## 금고 자리

```
keys/etc/<프로젝트>/<이름>/
  item.toml     기록 — 종류 · 용도 · 해시 · 소비처
  files/        들인 파일 그대로  0600
  values.env    그 파일을 여는 값  0600
```

## 들인 것 (2026-09-23)

| 항목 | 파일 | 값 |
|---|---|---|
| `tuk-app / android-upload` | `upload-keystore.jks` | `storePassword` · `keyPassword` · `keyAlias` |
| `gong-gugyeong / android-upload` | `upload-keystore.jks` | `storePassword` · `keyPassword` · `keyAlias` |
| `tuk / apple-api` | `AuthKey_XZRTQ6ZPQ8.p8` | — |

파일은 해시를 확인하며 옮겼고, 값은 `key.properties` 의 세 줄을 금고로 복사했다.
`key.properties` 는 빌드가 읽는 파일이라 제자리에 둔다.

옮긴 뒤 고쳐야 하는 설정:

| 파일 | 줄 |
|---|---|
| `11_tuk/03_tuk-app/android/key.properties` | `storeFile=/Users/david/.secrets/keys/etc/tuk-app/android-upload/files/upload-keystore.jks` |
| `61_gong-gugyeong/app/android/key.properties` | `storeFile=/Users/david/.secrets/keys/etc/gong-gugyeong/android-upload/files/upload-keystore.jks` |
| `11_tuk/02_tuk-api-server/.env.local` | `APPLE_ASC_PRIVATE_KEY_PATH="/Users/david/.secrets/keys/etc/tuk/apple-api/files/AuthKey_XZRTQ6ZPQ8.p8"` |

`.p8` 은 tukapp-prod · tukapp-dev 에도 사본이 있다. 서버가 쓰는 것이라 두고, 소비처로
기록한다. 2026-09-24 앱 실행 계정을 옮기며 서버의 자리가 `/home/deploy/secrets/` 로 바뀌었다.

## 구현

- 1단계 ✅ 2026-09-24 — 목록 · 상세를 금고의 실제 기록으로 보여 준다. 용도, 소비처 기록(넣고 빼기),
  여는 값 복사, Android `key.properties` 줄 복사. 코드: core `etc`, local `etc.rs`, gui `command/etc.rs`.
- 2단계 — 들이기(해시로 같은 파일 찾기 · 옮기기)와 보관소로 옮기기. 화면의 버튼도 그때 둔다.

## 시스템이 완성되면 정리할 것

- [ ] `~/workspace/upload-keystore.jks` — tuk-app 키스토어와 같은 파일(해시 일치). 흘린 사본
- [ ] `~/Downloads/AuthKey_XZRTQ6ZPQ8.p8` — `~/.secrets/tuk/` 의 것과 같은 파일. 받은 원본
- [ ] `~/Documents/psql-tunnel.pem` — Naver Cloud 서버용. 서버를 모두 걷어내 쓰지 않음
- [ ] `~/Documents/tuk/david-lee-admin_credentials.csv` — 관리자 IAM 콘솔 비밀번호 평문, 644. 유효 여부 미확인
- [ ] `80_my_projects/.05_market-analysis-backup-20260827/.secrets/` — API 키 `.env` 백업 사본 3개
- [x] `101_android-upload-key/gong-gugyeong/pwd.txt` — 키스토어 비밀번호 평문 사본. 금고 · key.properties 와 해시 일치 확인 후 2026-09-23 삭제
