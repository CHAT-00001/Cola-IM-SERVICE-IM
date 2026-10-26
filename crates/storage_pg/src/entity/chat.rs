// storage_pg/src/entity/chat.rs  -- 存储 - PG - 数据库映射实体 - 聊天表
// 2026-03-24 16:42

////////

use chrono::{DateTime, OutOfRange, Utc};
use sqlx::FromRow;
use uuid::Uuid;

////////

/// # [ENTITY] -聊天对话成员表
/// * `pg schema`: `im_2024`
/// * `table name`: `chat_session_members`
#[derive(Debug, FromRow)]
pub struct ChatMembersEntity {
    pub id: i64,                           // 聊天id 内部关联
    pub show_id: Uuid,                     // 对外展示
    pub user_id: i64,                      // 用户ID
    pub name: Option<String>,              // 聊天对话名称（对方昵称/备注  群名/备注）
    pub remark: Option<String>,            // 备注
    pub avatar_url: Option<String>,        // 头像url 群成员摘要头像url
    pub group_id: Uuid,                    // 群ID
    pub target_id: i64,                    // 目标用户id（单聊）
    pub last_message_id: Option<String>,   // 最后一条消息ID
    pub last_message_info: Option<String>, // 最后一条消息的摘要
    pub last_message_time: DateTime<Utc>,  // 最后一条消息时间
    pub chat_type: i16,                    // 聊天类型: 0. 公共 1. 私聊 2. 群聊 4. 系统消息
    pub unread_qty: i64,                   // 未读消息数量
    pub created_at: DateTime<Utc>,         // 创建时间
    pub updated_at: DateTime<Utc>,         // 更新时间
}

////////

/// # [ENTITY] - 聊天状态设置
/// * `pg schema`: `im_2024`
/// * `table name`: `chat_setting`
#[derive(Debug, FromRow)]
pub struct ChatStatusEntity {
    pub id: Uuid,                  // 聊天id uuid
    pub chat_id: i64,              // 关联聊天ID
    pub name: Option<String>,      // 聊天对话名称（对方昵称/备注  群名/备注）
    pub is_display: Option<bool>,  // 是否显示
    pub is_notice: Option<bool>,   // 通知开关
    pub is_mute: Option<bool>,     // 是否静音
    pub ringtone_id: Option<i64>,  // 铃声ID
    pub ringtone_vol: Option<i16>, // 铃声音量0-100
    pub created_at: DateTime<Utc>, // 创建时间
    pub updated_at: DateTime<Utc>, // 更新时间
}

////////

/// # 聊天对话统计数量实体
/// * `pg schema`: `im_2024`
/// * `table name`: `chat_qty`
#[derive(Debug, FromRow)]
pub struct ChatQtyEntity {
    pub id: Uuid,                  // 聊天id uuid
    pub chat_id: i64,              // chat ID
    pub message_qty: i64,          // 消息总数量
    pub inbox: i64,                // 收件箱总数量
    pub outbox: i64,               // 发件箱总数量
    pub video_qty: i64,            // 视频总数量
    pub voice_qty: i64,            // 语音数量
    pub created_at: DateTime<Utc>, // 创建时间
    pub updated_at: DateTime<Utc>, // 更新时间
}

//////// END