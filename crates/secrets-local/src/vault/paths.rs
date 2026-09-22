//! `~/.secrets` 안에서 계정이 차지하는 자리.
//!
//! 계정 하나가 디렉토리 하나다. 그 안에 계정 기록·CLI 격리 홈·교체 이력이 함께 있어,
//! 계정을 통째로 옮기면 자격과 이력이 같이 따라간다.

use std::path::{Path, PathBuf};

use secrets_core::account::{Account, Provider};

use crate::vault as home;

/// 계정 기록 파일의 이름.
pub const FILE: &str = "account.toml";
/// 교체 이력이 쌓이는 디렉토리.
pub const HISTORY: &str = "history";

/// 계정의 번들 디렉토리.
pub fn dir_of(provider: Provider, slug: &str) -> PathBuf {
    home::root()
        .join(home::ACCOUNTS)
        .join(provider.id())
        .join(slug)
}

/// 계정 전용 CLI 설정 홈.
pub fn cli_home_of(provider: Provider, slug: &str) -> PathBuf {
    dir_of(provider, slug).join("cli")
}

pub fn dir(account: &Account) -> PathBuf {
    dir_of(account.provider, &account.slug)
}

pub fn cli_home(account: &Account) -> PathBuf {
    cli_home_of(account.provider, &account.slug)
}

/// 주어진 CLI 홈을 가리키는 환경변수. 계정이 아직 없을 때도 쓴다.
///
/// 계정별 격리는 전부 이 값들로 이뤄진다. 계정을 바꾼다는 건 이 값들을 바꾼다는 뜻이다.
pub fn env_for(provider: Provider, home_dir: &Path) -> Vec<(&'static str, String)> {
    let home = home_dir.display().to_string();
    match provider {
        Provider::Github => vec![("GH_CONFIG_DIR", home)],
        Provider::Aws => vec![
            ("AWS_CONFIG_FILE", format!("{home}/config")),
            ("AWS_SHARED_CREDENTIALS_FILE", format!("{home}/credentials")),
        ],
        Provider::Gcloud => vec![("CLOUDSDK_CONFIG", home)],
        // firebase 는 XDG 규약을 따라 configstore 를 그 아래에 만든다.
        Provider::Firebase => vec![("XDG_CONFIG_HOME", home)],
    }
}

/// 이 계정으로 CLI 를 돌릴 때 덧씌울 환경변수.
pub fn env(account: &Account) -> Vec<(&'static str, String)> {
    env_for(account.provider, &cli_home(account))
}
