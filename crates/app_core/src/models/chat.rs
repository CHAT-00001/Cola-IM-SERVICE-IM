// app_core/src/models/chat.rs  -- 聊天列表模型
// 2026-10-11 16:20

////////

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

////////

/// # 聊天列表结构体
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct ChatListItem {
    pub chat_id: i64,                      // 聊天ID
    pub room_id: String,                   // 聊天室ID
    pub last_message_id: i64,              // 最后一条消息摘要
    pub updated_at: Option<DateTime<Utc>>, // 更新时间
    pub unread_count: Option<i64>,         // 未读消息条数
}

/// # [HANDLER] - ChatSessionView
/// * `DESC`: `单聊会话及当前请求用户的个性化设置`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatSessionView {
    pub session_id: String,
    pub channel: u8,
    pub status: i16,
    pub pinned: bool,
    pub last_message_summary: Option<String>,
    pub notification_settings: serde_json::Value,
    pub member_state: i16,
}

////////

/// # [MODEL] - ChatSessionListItem
/// * `DESC`: `聊天列表项，包含会话及当前账号的个性化视图`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatSessionListItem {
    pub session_id: String,
    pub channel: i16,
    pub status: i16,
    pub peer_account_id: i64,
    pub is_pinned: bool,
    pub last_message_id: Option<String>,
    pub last_message_summary: Option<String>,
    pub notification_settings: serde_json::Value,
    pub member_state: i16,
    pub unread_count: i64,
    pub updated_at: Option<DateTime<Utc>>,
}

//////// END
