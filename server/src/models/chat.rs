// src/models/chat.rs -- MODEL: Chat 聊天会话列表
// 2026-01-05 10:19:20

////////

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

////////

/// ## Chat 聊天列表
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Chat {
    pub id: String,                        // uuid v4
    pub chat_type: ChatType,               // 聊天类型
    pub rename: Option<String>,            // 重命名
    pub is_group: bool,                    // 是否群聊
    pub group_id: Option<u64>,             // 群聊 id
    pub last_message_id: Option<String>,   // 最后一条消息 id
    pub last_sender_id: Option<u64>,       // 最后一条消息发送者
    pub created_at: Option<DateTime<Utc>>, // 创建时间
    pub updated_at: Option<DateTime<Utc>>, // 更新时间
    pub unread_count: u32,                 // 未读消息数
    pub draft: Option<String>,             // 草稿
    pub pinned: bool,                      // 是否置顶
}

////////

/// ## Chat 类型
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum ChatType {
    Private = 0,    // 私聊
    Group = 1,      // 群聊
    PublicRoom = 2, // 公共聊天室
}

////////// END
