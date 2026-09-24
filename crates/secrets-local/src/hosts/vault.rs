//! 인스턴스 계정의 키와 기록이 놓이는 곳.
//!
//! ```text
//! keys/aws/<계정ID>/ec2/<리전>/<키페어>/
//!   key  key.toml                      pem — 이 아래 전부를 여는 마스터
//!   instance/<인스턴스ID>/<계정>/
//!     key  key.pub  key.toml
//! ```
//!
//! 인스턴스는 키페어의 자식이다. EC2 는 인스턴스의 키페어를 만든 뒤 바꿀 수 없어서
//! 이 관계는 끝까지 안 변한다.

use std::path::{Path, PathBuf};

use secrets_core::aws::instance::{HostError, InstanceAccount, InstanceVault, Seat};

use crate::aws_vault;
use crate::clock;
use crate::vault;

pub const FILE: &str = "key.toml";
pub const PRIVATE: &str = "key";
pub const PUBLIC: &str = "key.pub";

/// 이 금고가 어느 AWS 계정을 보고 있는가.
///
/// 계정 ID 는 경로의 첫 단계라, 자리를 잡으려면 먼저 알아야 한다. 화면이 확인
/// 단계에서 읽어 넘겨 준다.
pub struct FileAccounts {
    pub aws_account: String,
    pub machine: String,
}

fn storage(e: impl std::fmt::Display) -> HostError {
    HostError::Storage(e.to_string())
}

impl FileAccounts {
    pub fn dir_of(&self, seat: &Seat) -> PathBuf {
        aws_vault::dir_of(&self.aws_account, &self.machine, &seat.region, &seat.keypair)
            .join("instance")
            .join(&seat.instance)
            .join(&seat.account)
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), HostError> {
        use std::io::Write;

        let staging = path.with_extension("writing");
        {
            let mut file = std::fs::File::create(&staging).map_err(storage)?;
            file.write_all(bytes).map_err(storage)?;
            file.sync_all().map_err(storage)?;
        }
        vault::restrict(&staging).map_err(storage)?;
        std::fs::rename(&staging, path).map_err(storage)
    }
}

impl InstanceVault for FileAccounts {
    fn exists(&self, seat: &Seat) -> bool {
        self.dir_of(seat).join(FILE).is_file()
    }

    fn create(&self, seat: &Seat, comment: &str) -> Result<(String, String, String), HostError> {
        use crate::cli::{exec, tools};

        let at = self.dir_of(seat);
        vault::create_private(&at).map_err(storage)?;
        let target = at.join(PRIVATE);
        // 지난 시도가 남아 있으면 ssh-keygen 이 덮어쓸지 물으며 멈춘다.
        let _ = std::fs::remove_file(&target);
        let _ = std::fs::remove_file(at.join(PUBLIC));

        let program = tools::find_in_path("ssh-keygen")
            .ok_or_else(|| HostError::Storage("ssh-keygen 을 찾을 수 없습니다".into()))?;

        let outcome = exec::run(
            &program,
            &[
                "-t",
                "ed25519",
                "-C",
                comment,
                // 사람이 칠 수 없는 자리에서 쓰는 키다. 암호구를 걸면 배포가 멈춘다.
                "-N",
                "",
                "-f",
                &target.display().to_string(),
            ],
            |_, _| {},
        )
        .map_err(storage)?;

        if !outcome.ok() {
            return Err(HostError::Storage("키 쌍을 만들지 못했습니다".into()));
        }
        vault::restrict(&target).map_err(storage)?;

        let public = self.public_key(seat)?;
        let fingerprint = aws_vault::fingerprint_of(&target).map_err(storage)?;
        Ok((public, fingerprint, "ed25519".into()))
    }

    fn public_key(&self, seat: &Seat) -> Result<String, HostError> {
        std::fs::read_to_string(self.dir_of(seat).join(PUBLIC))
            .map(|text| text.trim_end().to_string())
            .map_err(|_| HostError::Missing(seat.slug()))
    }

    fn private_path(&self, seat: &Seat) -> String {
        self.dir_of(seat).join(PRIVATE).display().to_string()
    }

    fn record(&self, account: &InstanceAccount) -> Result<(), HostError> {
        let seat = Seat::new(
            &account.region,
            &account.keypair,
            &account.instance,
            &account.account,
        )
        .ok_or_else(|| HostError::Storage(format!("{} 자리를 읽지 못했습니다", account.account)))?;

        let at = self.dir_of(&seat);
        vault::create_private(&at).map_err(storage)?;
        let text = toml::to_string_pretty(account).map_err(storage)?;
        self.write(&at.join(FILE), text.as_bytes())
    }

    fn load(&self, seat: &Seat) -> Result<InstanceAccount, HostError> {
        let text = std::fs::read_to_string(self.dir_of(seat).join(FILE))
            .map_err(|_| HostError::Missing(seat.slug()))?;
        toml::from_str(&text).map_err(storage)
    }

    /// 디렉토리를 훑어 기록을 모은다. 읽지 못한 것은 건너뛰지 않고 오류로 남긴다.
    fn list(&self) -> Vec<Result<InstanceAccount, String>> {
        let mut found = Vec::new();
        let root = aws_vault::root().join(&self.aws_account).join(&self.machine);

        for region in dirs_in(&root) {
            for keypair in dirs_in(&root.join(&region)) {
                let under = root.join(&region).join(&keypair).join("instance");
                for instance in dirs_in(&under) {
                    for account in dirs_in(&under.join(&instance)) {
                        let Some(seat) = Seat::new(&region, &keypair, &instance, &account) else {
                            continue;
                        };
                        if !self.exists(&seat) {
                            continue;
                        }
                        found.push(
                            self.load(&seat)
                                .map_err(|e| format!("{}: {e}", seat.slug())),
                        );
                    }
                }
            }
        }
        found
    }

    fn archive(&self, seat: &Seat, reason: &str) -> Result<(), HostError> {
        let stamp = clock::stamp();
        let kept = vault::root()
            .join("archive")
            .join("keys")
            .join("aws")
            .join(&self.aws_account)
            .join(&self.machine)
            .join(&seat.region)
            .join(&seat.keypair)
            .join("instance")
            .join(&seat.instance)
            .join(format!("{}-{stamp}", seat.account));

        if let Some(parent) = kept.parent() {
            vault::create_private(parent).map_err(storage)?;
        }

        let at = self.dir_of(seat);
        // 무엇을 왜 걷어냈는지 남긴다. 이유 없는 보관은 나중에 판단할 수 없다.
        let note = format!("걷어낸 시각 = \"{}\"\n이유 = \"{reason}\"\n", clock::now());
        let _ = std::fs::write(at.join("archived.toml"), note);

        std::fs::rename(&at, &kept).map_err(storage)
    }

    fn discard(&self, seat: &Seat) {
        let at = self.dir_of(seat);
        let _ = std::fs::remove_dir_all(&at);
        // 이 자리 때문에 생긴 빈 `<인스턴스>/` · `instance/` 도 걷는다. 비어 있지 않으면
        // remove_dir 가 실패하고 그대로 남는다.
        for parent in at.ancestors().skip(1).take(2) {
            if std::fs::remove_dir(parent).is_err() {
                break;
            }
        }
    }
}

fn dirs_in(path: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}
