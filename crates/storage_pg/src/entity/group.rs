// storage_pg/src/entity/group.rs  -- 存储 - PG - entity - 群表
// 2026-03-24 16:42

////////

use chrono::{DateTime, OutOfRange, Utc};
use sqlx::FromRow;
use uuid::Uuid;

////////

/// # [ENTITY] - 群表
/// * `pg schema`: `im_2024`
/// * `table name`: `group`
#[derive(Debug, FromRow)]
pub struct GroupEntity {
    pub id: i64,                   // 群id（内部关联）
    pub show_id: Uuid,             // 群id（外部展示）
    pub status: i16,               // 状态: 0. 草稿 1. 正在发送 2. 服务器存储 3. 收件
    pub room_id: Uuid,             // 房间id（公共聊天）
    pub user_id: i64,              // 用户id
    pub session_id: i64,           // 会话id
    pub name: Option<String>,      // 群名称
    pub cname: Option<String>,     // 群名称（别名）
    pub desc: Option<String>,      // 描述
    pub qty: i64,                  // 群成员数量
    pub group_type: Option<i16>,   // 类型
    pub created_at: DateTime<Utc>, // 创建时间
    pub updated_at: DateTime<Utc>, // 更新时间
}

////////

/// # [ENTITY] - 群用户表
/// * `pg schema`: `im_2024`
/// * `table name`: `group_user`
#[derive(Debug, FromRow)]
pub struct GroupUserEntity {
    pub id: Uuid,                      // 群id
    pub status: i16,                   // 状态: 0. 失效 1. 正常
    pub group_id: i64,                 // 房间id（公共聊天）
    pub user_id: i64,                  // 用户id
    pub target_id: i64,                // 接收者id
    pub tip_name: Option<String>,      // 群名片名称（可选）
    pub remark: Option<String>,        // 群备注
    pub send_message_qty: Option<i64>, // 发送消息数量
    pub msg_type: i16,                 // 消息类型
    pub apply_at: DateTime<Utc>,       // 申请加入时间
    pub added_at: DateTime<Utc>,       // 加入时间（如果群未开启审核加入，则自动加入）
    pub sync_at: DateTime<Utc>,        // 同步时间（服务端生成）
}

//////// END