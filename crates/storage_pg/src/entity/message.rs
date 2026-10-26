// storage_pg/src/entity/message.rs  -- 数据库映射
// 2026-03-24 16:42

////////

use chrono::{DateTime, OutOfRange, Utc};
use sqlx::FromRow;
use uuid::Uuid;

////////

/// # [ENTITY] - 消息
/// * `DESC`: `MongoDB`
#[derive(Debug, FromRow)]
pub struct MessageEntity {
    pub id: Uuid,                           // 消息id - 客户端生成uuid v4
    pub status: i16,                        // 状态: 0. 草稿 1. 正在发送 2. 服务器存储 3. 收件
    pub room_id: Uuid,                      // 房间id（公共聊天）
    pub sender_id: i64,                     // 发送者id
    pub target_id: i64,                     // 接收者id
    pub content: String,                    // 系统内容
    pub payload: Option<serde_json::Value>, // 装载内容
    pub at_message_id: Option<String>,      // 引用的消息（可选）
    pub msg_type: i16,                      // 消息类型
    pub send_at: DateTime<Utc>,             // 发送时间（客户端生成）
    pub sync_at: DateTime<Utc>,             // 同步时间（服务端生成）
}

////////

/// # [ENTITY] - 发件箱
/// * `DESC`: `MongoDB`
#[derive(Debug, FromRow)]
pub struct MessageOutboxEntity {
    pub id: Uuid,                           // 消息id - 客户端生成uuid v4
    pub status: i16,                        // 状态: 0. 草稿 1. 正在发送 2. 服务器存储 3. 收件
    pub room_id: Option<Uuid>,              // 房间id（公共聊天）
    pub sender_id: i64,                     // 发送者id
    pub target_id: i64,                     // 接收者id
    pub content: String,                    // 系统内容
    pub payload: Option<serde_json::Value>, // 装载内容
    pub at_message_id: Option<String>,      // 引用的消息（可选）
    pub msg_type: i16,                      // 消息类型
    pub send_at: DateTime<Utc>,             // 发送时间（客户端生成）
    pub sync_at: DateTime<Utc>,             // 同步时间（服务端生成）
}

////////

/// # [ENTITY] - 收件箱
/// * `DESC`: `MongoDB`
#[derive(Debug, FromRow)]
pub struct MessageInboxEntity {
    pub id: Uuid,                           // 消息id - 客户端生成uuid v4
    pub status: i16,                        // 状态: 0. 草稿 1. 正在发送 2. 服务器存储 3. 收件
    pub room_id: Uuid,                      // 房间id（公共聊天）
    pub sender_id: i64,                     // 发送者id
    pub target_id: i64,                     // 接收者id
    pub content: String,                    // 系统内容
    pub payload: Option<serde_json::Value>, // 装载内容
    pub at_message_id: Option<String>,      // 引用的消息（可选）
    pub msg_type: i16,                      // 消息类型
    pub send_at: DateTime<Utc>,             // 发送时间（客户端生成）
    pub sync_at: DateTime<Utc>,             // 同步时间（服务端生成）
}

//////// END