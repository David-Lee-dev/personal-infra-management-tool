//! 물려 둔 것과, 물린 이유.
//!
//! 이 도구는 지우지 않고 물린다. 그래서 물린 이유가 남아야 나중에 "왜 이게 여기
//! 있나" 를 답할 수 있다.

use serde::{Deserialize, Serialize};

/// 무엇 때문에 아카이브했는가.
///
/// 이 도구는 지우지 않고 물린다. 그래서 물린 이유가 남아야 나중에
/// "왜 이게 여기 있나" 를 답할 수 있다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArchiveReason {
    /// 자격을 새 것으로 갈아 끼웠다. 계정은 그대로 남는다.
    Replaced,
    /// 계정을 목록에서 내렸다. 계정 전체가 물러난다.
    Deleted,
}

impl ArchiveReason {
    pub fn id(&self) -> &'static str {
        match self {
            ArchiveReason::Replaced => "replaced",
            ArchiveReason::Deleted => "deleted",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ArchiveReason::Replaced => "교체",
            ArchiveReason::Deleted => "삭제",
        }
    }
}

/// 자격을 교체한 기록. 값은 담지 않는다.
///
/// 구 토큰은 GitHub 에서 재발급하는 순간 죽으므로 보관해도 복구에 쓸 수 없다.
/// 남길 값어치가 있는 건 "언제 무엇을 왜 바꿨나" 쪽이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replacement {
    pub replaced_at: String,
    pub reason: ArchiveReason,
    /// 교체를 부른 사정. `만료됨` 처럼 사람이 읽을 한 줄.
    #[serde(default)]
    pub detail: String,
    /// 교체 직전의 신원. 같은 계정으로 바꿨는지 나중에 확인할 수 있다.
    #[serde(default)]
    pub identity: String,
    #[serde(default)]
    pub expires: Option<String>,
    #[serde(default)]
    pub verified_at: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
}
