// src/models/group.rs
// 2026-01-05 10:19:20
use chrono::{DateTime, Utc};

/// Chat 聊天列表
pub struct Group {
    pub id: String,                        // id
    pub name: Option<String>,              // 重命名
    pub is_public: bool,                   // 是否公开，直播聊天时需要开启
    pub group_id: Option<u64>,             // 群聊 id
    pub room_id: Option<u64>,              // 房间 id
    pub stream: Option<String>,            // 直播流名称
    pub last_message_id: Option<String>,   // 最后一条消息 id
    pub last_sender_id: Option<u64>,       // 最后一条消息发送者
    pub created_at: Option<DateTime<Utc>>, // 创建于
    pub updated_at: Option<DateTime<Utc>>, // 更新于
    pub unread_count: u32,
    pub draft: Option<String>,
    pub pinned: bool,
}
