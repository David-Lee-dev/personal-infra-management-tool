//! 번들 스캔, meta.toml 파싱, 검증, 회전 계획 수립.
//! GUI 와 CLI 가 공유하는 유일한 로직 계층. 비밀값은 이 크레이트 밖으로 나가지 않는다.

pub const ROOT_ENV: &str = "SECRETS_HOME";
pub const ROOT_DEFAULT: &str = "~/.secrets";
