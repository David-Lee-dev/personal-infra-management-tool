//! 개인 키를 금고로 들이는 규칙.
//!
//! AWS 는 개인 키를 다시 주지 않는다. 엉뚱한 키를 들이면 나중에 알아차릴 방법이
//! 없으므로, 지문이 맞을 때만 들이는지를 본다.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use secrets_core::aws::{AwsError, KeyPairRecord};
use secrets_local::aws_vault;

/// 금고 뿌리는 프로세스 전역이라 테스트가 겹치면 서로를 덮는다.
static ALONE: Mutex<()> = Mutex::new(());

struct Scratch {
    root: PathBuf,
    _held: MutexGuard<'static, ()>,
}

impl Scratch {
    fn new(name: &str) -> Scratch {
        let held = ALONE.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("secrets-aws-vault-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        unsafe { std::env::set_var("SECRETS_HOME", &root) };
        Scratch { root, _held: held }
    }

    /// 실제 키 쌍을 만든다. 지문이 진짜여야 검사에 의미가 있다.
    fn keygen(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        let done = std::process::Command::new("ssh-keygen")
            .args(["-t", "ed25519", "-N", "", "-C", name, "-f"])
            .arg(&path)
            .output()
            .expect("ssh-keygen");
        assert!(done.status.success(), "ssh-keygen 실패");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
        unsafe { std::env::remove_var("SECRETS_HOME") };
    }
}

fn record(name: &str) -> KeyPairRecord {
    KeyPairRecord {
        name: name.to_string(),
        account: "320042238085".into(),
        machine: "ec2".into(),
        region: "ap-northeast-2".into(),
        fingerprint: String::new(),
        purpose: String::new(),
        verified: false,
        adopted_at: String::new(),
        expires: None,
    }
}

fn aws_form(fingerprint: &str) -> String {
    // AWS 는 접두사 없이 패딩을 붙여 적는다.
    format!("{}=", fingerprint.trim_start_matches("SHA256:"))
}

#[test]
fn a_matching_private_key_is_taken_in_and_recorded() {
    let scratch = Scratch::new("match");
    let pem = scratch.keygen("tuk-key");
    let mine = aws_vault::fingerprint_of(&pem).unwrap();

    let kept = aws_vault::adopt(record("tuk-key"), &pem, Some(&aws_form(&mine))).unwrap();

    assert_eq!(kept.fingerprint, mine);
    assert!(!kept.adopted_at.is_empty());

    let at = aws_vault::dir_of("320042238085", "ec2", "ap-northeast-2", "tuk-key");
    assert!(at.join(aws_vault::PRIVATE).is_file(), "개인 키가 제자리에 있어야 한다");
    assert_eq!(aws_vault::load("320042238085", "ec2", "ap-northeast-2", "tuk-key").unwrap().name, "tuk-key");
}

#[test]
fn a_private_key_from_a_different_pair_is_refused_and_nothing_is_written() {
    let scratch = Scratch::new("mismatch");
    let mine = scratch.keygen("mine");
    let other = scratch.keygen("other");
    let expected = aws_form(&aws_vault::fingerprint_of(&other).unwrap());

    let refused = aws_vault::adopt(record("tuk-key"), &mine, Some(&expected));
    assert!(matches!(refused, Err(AwsError::Mismatch { .. })));

    // 거절했으면 아무것도 남지 않아야 한다. 반쯤 들인 키가 제일 나쁘다.
    let at = aws_vault::dir_of("320042238085", "ec2", "ap-northeast-2", "tuk-key");
    assert!(!at.join(aws_vault::PRIVATE).exists());
    assert!(!at.join(aws_vault::FILE).exists());
}

#[test]
fn a_key_already_in_the_vault_is_not_overwritten() {
    let scratch = Scratch::new("taken");
    let first = scratch.keygen("first");
    aws_vault::adopt(
        record("tuk-key"),
        &first,
        Some(&aws_form(&aws_vault::fingerprint_of(&first).unwrap())),
    )
    .unwrap();

    // 같은 자리에 다른 키를 들이려 한다.
    let second = scratch.keygen("second");
    let expected = aws_form(&aws_vault::fingerprint_of(&second).unwrap());
    let again = aws_vault::adopt(record("tuk-key"), &second, Some(&expected));

    assert!(matches!(again, Err(AwsError::Taken(_))));
    // 거절했으면 두 번째 원본도 그대로여야 한다. 지우면 그 키를 잃는다.
    assert!(second.is_file());
}

#[test]
fn a_fingerprint_aws_cannot_express_is_never_accepted() {
    let scratch = Scratch::new("md5");
    let pem = scratch.keygen("tuk-key");

    // AWS 가 가져온 RSA 키페어에 주는 형태. 비교할 수 없으므로 들이면 안 된다.
    let md5 = "1f:51:ae:28:bf:89:e9:d8:1f:25:5d:37:2d:7d:b8:ca";
    assert!(matches!(
        aws_vault::adopt(record("tuk-key"), &pem, Some(md5)),
        Err(AwsError::Mismatch { .. })
    ));
}

#[test]
fn the_vault_lists_what_it_holds() {
    let scratch = Scratch::new("list");
    for name in ["alpha", "beta"] {
        let pem = scratch.keygen(name);
        let expected = aws_form(&aws_vault::fingerprint_of(&pem).unwrap());
        aws_vault::adopt(record(name), &pem, Some(&expected)).unwrap();
    }

    let held: Vec<String> = aws_vault::list()
        .into_iter()
        .map(|entry| entry.unwrap().name)
        .collect();
    assert_eq!(held, vec!["alpha", "beta"]);
}

// 경로가 실제로 쓰이는 자리를 가리키는지.
#[test]
fn the_path_says_whose_key_it_is() {
    let _scratch = Scratch::new("path");
    let at = aws_vault::dir_of("320042238085", "ec2", "ap-northeast-2", "tuk-key");
    let text = at.display().to_string();
    assert!(text.contains("keys/aws/320042238085/ec2/ap-northeast-2/tuk-key"), "{text}");
}

#[test]
fn adopting_always_takes_the_original_and_its_public_half() {
    let scratch = Scratch::new("move");
    let pem = scratch.keygen("tuk-key");
    let public = pem.with_extension("pub");
    let expected = aws_form(&aws_vault::fingerprint_of(&pem).unwrap());

    let kept = aws_vault::adopt(record("tuk-key"), &pem, Some(&expected)).unwrap();

    assert!(!pem.exists(), "들였으면 있던 자리에 남으면 안 된다");
    assert!(!public.exists(), "짝을 잃은 공개 키만 남겨 두지 않는다");
    assert!(kept.verified);

    // 금고 쪽은 온전해야 한다. 원본을 지웠으므로 이것이 유일한 사본이다.
    let at = aws_vault::dir_of("320042238085", "ec2", "ap-northeast-2", "tuk-key")
        .join(aws_vault::PRIVATE);
    assert_eq!(aws_vault::fingerprint_of(&at).unwrap(), kept.fingerprint);
}

#[test]
fn a_refused_adoption_never_touches_the_original() {
    let scratch = Scratch::new("refused");
    let mine = scratch.keygen("mine");
    let other = scratch.keygen("other");
    let expected = aws_form(&aws_vault::fingerprint_of(&other).unwrap());

    assert!(aws_vault::adopt(record("tuk-key"), &mine, Some(&expected)).is_err());

    // 지문이 안 맞아 거절했는데 원본이 사라지면 키를 잃는다.
    assert!(mine.is_file());
}

#[test]
fn a_key_aws_cannot_vouch_for_is_taken_but_marked_unverified() {
    let scratch = Scratch::new("unverified");
    let pem = scratch.keygen("LightsailDefaultKeyPair");

    // AWS 가 지문을 주지 않는 키페어가 있다. 들이되 확인한 척하지 않는다.
    let kept = aws_vault::adopt(record("LightsailDefaultKeyPair"), &pem, None).unwrap();

    assert!(!kept.verified, "확인하지 못한 것을 확인했다고 적으면 안 된다");
    assert!(!kept.fingerprint.is_empty(), "우리가 읽은 지문은 남긴다");
    assert!(!pem.exists());
}

#[test]
fn an_rsa_pem_is_also_written_the_ways_aws_writes_rsa_key_pairs() {
    let scratch = Scratch::new("rsa-digests");
    let pem = scratch.root.join("rsa.pem");
    let made = std::process::Command::new("openssl")
        .args(["genrsa", "-traditional", "-out"])
        .arg(&pem)
        .arg("2048")
        .output()
        .expect("openssl");
    assert!(made.status.success(), "openssl genrsa 실패");

    let all = aws_vault::fingerprints_of(&pem).unwrap();

    assert!(all[0].starts_with("SHA256:"));
    let colon = |bytes: usize| all.iter().filter(|f| f.split(':').count() == bytes).count();
    assert_eq!(colon(16), 1, "공개 키 DER 의 MD5 가 있어야 한다: {all:?}");
    assert_eq!(colon(20), 1, "PKCS#8 DER 의 SHA1 이 있어야 한다: {all:?}");
}

#[test]
fn an_openssh_key_has_only_its_sha256() {
    let scratch = Scratch::new("openssh-digests");
    let pem = scratch.keygen("ed");

    let all = aws_vault::fingerprints_of(&pem).unwrap();

    assert_eq!(all, vec![aws_vault::fingerprint_of(&pem).unwrap()], "빈 입력의 다이제스트가 끼면 안 된다");
}
