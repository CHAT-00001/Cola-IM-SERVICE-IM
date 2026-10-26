// storage_pg/src/repository/message.rs
// 2026-03-23 10:45

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row}; // 引入 Row 用于手动取值
use uuid::Uuid;

use app_core::models::chat::ChatListItem;
use app_core::models::message::{Event, Message, MessageQuery, MsgType, Payload, Status};
use app_core::ports::message::MessageRepository;
// 确保你的 Entity 定义中使用了 Option<serde_json::Value>
use crate::entity::message::MessageEntity;

use super::log_sqlx_error;

pub struct PgMessageRepository {
    pool: PgPool,
}

impl PgMessageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn clamp_limit(limit: u32) -> i64 {
        limit.clamp(1, 100) as i64
    }
}

#[async_trait]
impl MessageRepository for PgMessageRepository {
    /// 发件箱持久化 - 已去掉 sqlx::query! 宏
    async fn save_message(&self, msg: &Message) -> Result<String, String> {
        let msg_id = Uuid::parse_str(&msg.send_id).map_err(|e| format!("invalid uuid: {}", e))?;
        let room_id = Uuid::parse_str(&msg.room_id).map_err(|e| format!("invalid uuid: {}", e))?;
        let sender_id = msg.uid as i64;

        let payload_json = serde_json::to_value(&msg.payload).ok();

        // 使用运行时 query，不再受编译期编码错误困扰
        let row = sqlx::query(
            r#"
            INSERT INTO messages
                (id, room_id, sender_id, content, payload, msg_type, created_at)
            VALUES
                ($1, $2, $3, $4, $5, $6, NOW())
            ON CONFLICT (id) DO NOTHING
            RETURNING id
            "#,
        )
        .bind(msg_id)
        .bind(room_id)
        .bind(sender_id)
        .bind(payload_json)
        .bind(msg.msg_type.clone() as i16) // 将枚举转为 i16 存储
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            log_sqlx_error("message.save_message", &e);
            format!("db error: {}", e)
        })?;

        let id_str = if let Some(r) = row {
            let id: Uuid = r.get(0); // 获取 RETURNING id
            id.to_string()
        } else {
            msg.send_id.clone()
        };

        Ok(id_str)
    }

    /// 增量消息拉取 - 处理 MsgType 和 Payload 转换
    async fn get_messages_after(&self, query: MessageQuery) -> Result<Vec<Message>, String> {
        let limit: i64 = Self::clamp_limit(query.limit.try_into().unwrap_or(20));
        let room_id =
            Uuid::parse_str(&query.room_id).map_err(|e| format!("invalid uuid: {}", e))?;

        let rows = sqlx::query_as::<_, MessageEntity>(
            r#"
            SELECT id, room_id, sender_id, content, payload, msg_type, sync_at
            FROM messages
            WHERE room_id = $1 AND created_at > $2
            ORDER BY created_at ASC
            LIMIT $3
            "#,
        )
        .bind(room_id)
        .bind(query.last_msg_time)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            log_sqlx_error("message.get_messages_after", &e);
            format!("db error: {}", e)
        })?;

        let messages: Vec<Message> = rows
            .into_iter()
            .map(|r| {
                // 解析 JSON Payload
                let payload = r
                    .payload
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or(Payload::Content {
                        text: "".to_string(),
                    });

                // 显式转换 MsgType (i16 -> Enum)
                let msg_type = match r.msg_type {
                    1 => MsgType::Text,
                    2 => MsgType::Image,
                    _ => MsgType::Text,
                };

                Message {
                    id: r.id.to_string(),
                    send_id: r.id.to_string(),
                    sync_id: "".to_string(),
                    status: Status::Syncing,
                    client_id: "".to_string(),
                    send_at: r.sync_at,
                    server_sync_at: Some(Utc::now()),
                    msg_type,
                    event: Event::SendMsg,
                    room_id: r.room_id.to_string(),
                    uid: r.sender_id as u64,
                    sender: None,
                    payload,
                    channel: None,
                }
            })
            .collect();

        Ok(messages)
    }

    /// 获取聊天列表增量 - 解决 ChatListItem 类型不匹配问题
    async fn get_chat_list_incremental(
        &self,
        user_id: &str,
        last_t: DateTime<Utc>,
    ) -> Result<Vec<ChatListItem>, String> {
        // 关键：这里不直接 query_as::<_, ChatListItem>，避免版本冲突报错
        // 我们先通过原始 query 获取 Row，再手动创建 ChatListItem
        let rows = sqlx::query(
            r#"
            SELECT room_id, last_message, updated_at, unread_count
            FROM chat_members
            WHERE user_id = $1 AND updated_at > $2
            ORDER BY updated_at DESC
            LIMIT 100
            "#,
        )
        .bind(user_id)
        .bind(last_t)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            log_sqlx_error("message.get_chat_list_incremental", &e);
            format!("db error: {}", e)
        })?;

        let items: Vec<ChatListItem> = rows
            .into_iter()
            .map(|row| {
                ChatListItem {
                    // 根据你的 ChatListItem 字段名和类型调整
                    chat_id: 0,
                    room_id: row.get::<Uuid, _>("room_id").to_string(),
                    last_message_id: row.get::<Option<i64>, _>("last_message").unwrap_or(0),
                    updated_at: Some(row.get::<DateTime<Utc>, _>("updated_at")),
                    unread_count: Some(row.get::<i64, _>("unread_count")),
                }
            })
            .collect();

        Ok(items)
    }
}
