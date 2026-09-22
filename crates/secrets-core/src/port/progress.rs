//! 오래 걸리는 일이 진행 중임을 알리는 곳.
//!
//! 자격 증명 도구가 무엇을 하는지 보이지 않으면 믿을 근거가 없다. 그래서 실행 과정은
//! 사람이 보는 창으로 그대로 흐른다.

/// 진행 상황이 나가는 줄. 사람이 보고 있는 창에 그대로 흐른다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Out,
    Err,
}

/// 오래 걸리는 일이 진행 중임을 알리는 곳.
pub trait ProgressSink: Send + Sync {
    fn line(&self, channel: Channel, text: &str);
}

/// 아무 데도 보내지 않는다. 조용히 돌려야 하는 자리와 테스트에 쓴다.
pub struct Silent;

impl ProgressSink for Silent {
    fn line(&self, _channel: Channel, _text: &str) {}
}

