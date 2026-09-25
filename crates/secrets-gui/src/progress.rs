//! 실행 중인 일을 화면에 알리는 통로.
//!
//! 이 앱이 실행하는 것 중 사용자에게 숨기는 명령은 없다. 모든 실행이 이 이벤트로
//! 터미널 패널에 그대로 흐른다.

use secrets_core::port;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
pub struct Started {
    /// 이 실행을 가리키는 id. 프론트가 완료 이벤트와 짝지을 때 쓴다.
    pub job: String,
    pub command: String,
}

#[derive(Clone, Serialize)]
pub struct Line {
    pub job: String,
    pub stream: &'static str,
    pub line: String,
}

#[derive(Clone, Serialize)]
pub struct Ended {
    pub job: String,
    pub ok: bool,
    pub message: String,
}

/// 터미널 패널로 진행 상황을 흘리는 관찰자.
pub struct JobPanel {
    pub app: AppHandle,
    pub job: String,
}

impl port::ProgressSink for JobPanel {
    fn line(&self, channel: port::Channel, text: &str) {
        let _ = self.app.emit(
            "cli:line",
            Line {
                job: self.job.clone(),
                stream: match channel {
                    port::Channel::Out => "out",
                    port::Channel::Err => "err",
                    port::Channel::Step => "step",
                },
                line: text.to_string(),
            },
        );
    }
}

pub fn next_job_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    format!("job-{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}
