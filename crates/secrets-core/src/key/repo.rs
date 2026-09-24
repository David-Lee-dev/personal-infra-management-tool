//! 리포지토리 지목.
//!
//! 사람이 손에 들고 있는 것은 git 주소다. 소유자와 이름을 따로 치게 하면 그때마다
//! 옮겨 적는 실수가 생기므로, 주소를 그대로 받아 여기서 읽는다.

use serde::{Deserialize, Serialize};

/// `owner/repo`. 배포 키가 붙는 자리다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoRef {
    owner: String,
    name: String,
}

impl RepoRef {
    /// git 주소나 `owner/repo` 를 읽는다. 읽지 못하면 `None`.
    ///
    /// 읽지 못한 것을 추측해서 채우지 않는다 — 엉뚱한 리포에 키를 심는 것이
    /// 못 만드는 것보다 훨씬 나쁘다.
    pub fn parse(text: &str) -> Option<RepoRef> {
        let trimmed = text.trim().trim_end_matches('/');
        let body = trimmed
            .strip_prefix("git@github.com:")
            .or_else(|| trimmed.strip_prefix("https://github.com/"))
            .or_else(|| trimmed.strip_prefix("ssh://git@github.com/"))
            .unwrap_or(trimmed);

        let body = body.strip_suffix(".git").unwrap_or(body);
        let (owner, name) = body.split_once('/')?;

        if !valid(owner) || !valid(name) {
            return None;
        }
        Some(RepoRef {
            owner: owner.to_string(),
            name: name.to_string(),
        })
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// `owner/repo`. GitHub API 경로이자 화면에 보이는 형태다.
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

/// GitHub 이 허용하는 글자만. 경로 조각이 되므로 `.` 로만 이뤄진 것도 막는다.
fn valid(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shape_of_github_address_reads_as_the_same_repo() {
        let expected = RepoRef::parse("david-lee-dev/nemo-play").unwrap();
        for text in [
            "git@github.com:david-lee-dev/nemo-play.git",
            "git@github.com:david-lee-dev/nemo-play",
            "https://github.com/david-lee-dev/nemo-play",
            "https://github.com/david-lee-dev/nemo-play.git",
            "https://github.com/david-lee-dev/nemo-play/",
            "ssh://git@github.com/david-lee-dev/nemo-play.git",
            "  david-lee-dev/nemo-play  ",
        ] {
            assert_eq!(RepoRef::parse(text).as_ref(), Some(&expected), "{text}");
        }
    }

    #[test]
    fn what_cannot_be_read_is_refused_rather_than_guessed() {
        for text in [
            "",
            "   ",
            "nemo-play",
            "그냥 글자",
            "david-lee-dev/",
            "/nemo-play",
            "a/b/c",
            "../etc/passwd",
            "david-lee-dev/..",
        ] {
            assert_eq!(RepoRef::parse(text), None, "{text}");
        }
    }

    #[test]
    fn a_repo_names_itself_the_way_github_does() {
        let repo = RepoRef::parse("git@github.com:tuk-tuk-im/tuk-app.git").unwrap();
        assert_eq!(repo.slug(), "tuk-tuk-im/tuk-app");
        assert_eq!(repo.owner(), "tuk-tuk-im");
        assert_eq!(repo.name(), "tuk-app");
    }
}
