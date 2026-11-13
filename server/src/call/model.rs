// server/src/call/model.rs -- 通话领域模型
// 2026-11-12 16:40 Created.
//
// 端到端 WebRTC：媒体不经过服务器，服务端只维护通话状态机并转发 SDP/ICE。
// 中央推流（SFU / stream-server / 第三方）以 MediaPlane 预留，见 media.rs。

////////

use chrono::{DateTime, Utc};

////////

/// # [MODEL] - 通话形态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallKind {
    Voice,
    Video,
}

impl CallKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Voice => "voice",
            Self::Video => "video",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "voice" => Some(Self::Voice),
            "video" => Some(Self::Video),
            _ => None,
        }
    }
}

////////

/// # [MODEL] - 通话状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallState {
    Ringing,
    Connected,
    Ended,
}

impl CallState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ringing => "ringing",
            Self::Connected => "connected",
            Self::Ended => "ended",
        }
    }
}

////////

/// # [MODEL] - 结束原因
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    Missed,
    Rejected,
    Cancelled,
    Hangup,
    Failed,
}

impl EndReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Missed => "missed",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
            Self::Hangup => "hangup",
            Self::Failed => "failed",
        }
    }
}

////////

/// # [MODEL] - 一次通话的运行时状态
/// * `DESC`: `只存在于内存；落库的持久记录是 Call 消息本身`
#[derive(Debug, Clone)]
pub struct CallRuntime {
    pub call_id: String,
    pub room_id: String,
    pub caller_uid: u64,
    pub callee_uid: u64,
    pub caller_device: String,
    /// 主叫的 offer SDP：被叫晚到/丢帧时也能凭它接听（轮询兜底）
    pub offer_sdp: Option<String>,
    pub kind: CallKind,
    pub state: CallState,
    pub created_at: DateTime<Utc>,
    pub answered_at: Option<DateTime<Utc>>,
    pub end_reason: Option<EndReason>,
}

impl CallRuntime {
    /// # [HELPER] - 是否参与方
    pub fn is_participant(&self, uid: u64) -> bool {
        uid == self.caller_uid || uid == self.callee_uid
    }

    /// # [HELPER] - 接通后的毫秒数
    pub fn duration_ms(&self, now: DateTime<Utc>) -> u64 {
        match self.answered_at {
            Some(answered) => (now - answered).num_milliseconds().max(0) as u64,
            None => 0,
        }
    }
}

//////// END
