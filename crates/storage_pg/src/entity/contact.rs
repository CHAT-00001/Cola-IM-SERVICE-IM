// storage_pg/src/entity/contact.rs  -- 联系人 数据库映射
// 2026-03-24 16:42

////////

use chrono::{DateTime, OutOfRange, Utc};
use sqlx::FromRow;
use uuid::Uuid;

////////

/// # [ENTITY] - 联系人资料表
/// * `pg schema`: `im_2024`
/// * `table name`: `contact_profile`
#[derive(Debug, FromRow)]
pub struct ContactProfileEntity {
    pub id: i64,                         // id（内部关联）
    pub show_id: Uuid,                   // 群id（外部展示）
    pub status: i16,                     // 状态: 0. 失效 1. 正常 2. 异常 3. baned
    pub name: String,                    // 名称
    pub name_zh: Option<String>,         // 中文名称
    pub name_en: Option<String>,         // 英文名称
    pub first_name: Option<String>,      // 第一名称（姓）
    pub last_name: Option<String>,       // 第二名称（名）
    pub avatar_url: Option<String>,      // 头像url
    pub gender: Option<i16>,             // 性别类型枚举
    pub birthday: Option<DateTime<Utc>>, // 出生年月日
    pub telephone: Option<String>,       // 电话
    pub email: Option<String>,           // 邮箱
    pub wechat: Option<String>,          // 微信
    pub whatsapp: Option<String>,        // WhatsApp
    pub telegram: Option<String>,        // telegram 电报号
    pub session_id: i64,                 // 会话id
    pub hometown: Option<String>,        // 原生产自/家乡
    pub occupation: Option<String>,      // 职业（学生/教师/自由职业）
    pub description: Option<String>,     // 描述
    pub location_id: Option<i64>,        // 现位于
    pub contact_type: Option<i16>,       // 类型: 0. 个人 1. 企业 3. 官府 4. ...
    pub created_at: DateTime<Utc>,       // 创建时间
    pub updated_at: DateTime<Utc>,       // 更新时间
}

////////

/// # [ENTITY] - 联系人状态（联系人设置）
/// * `pg schema`: `im_2024`
/// * `table name`: `contact_status`
#[derive(Debug, FromRow)]
pub struct ContactStatusEntity {
    pub contact_id: i64,           // 关联联系人id
    pub status: i16,               // 状态: 0. 失效 1. 正常
    pub is_discover: Option<bool>, // 是否可被发现（关闭后联系人不可被未知对象发现）
    pub is_apply: Option<bool>,    // 是否申请添加（默认需要申请）
    pub created_at: DateTime<Utc>, // 创建时间
    pub updated_at: DateTime<Utc>, // 更新时间
}

////////

/// # [ENTITY] - 联系人列表（通信录）
/// * `pg schema`: `im_2024`
/// * `table name`: `contact_list`
#[derive(Debug, FromRow)]
pub struct ContactListEntity {
    pub id: i64,                           // 通信录id
    pub status: i16,                       // 状态: 0. 失效 1. 正常
    pub user_id: i64,                      // 用户ID
    pub target_id: i64,                    // 目标ID
    pub alias: Option<String>,             // 名片名称（可选）
    pub remark: Option<String>,            // 群备注
    pub is_starred: Option<bool>,          // 是否星标
    pub is_deleted: Option<bool>,          // 是否删除
    pub contact_type: Option<i16>,         // 联系人类型
    pub apply_at: Option<DateTime<Utc>>,   // 申请添加时间
    pub added_at: Option<DateTime<Utc>>,   // 添加时间（如果对方未开启审核加入，则自动加入）
    pub sync_at: DateTime<Utc>,            // 同步时间（服务端生成）
    pub created_at: DateTime<Utc>,         // 创建时间
    pub updated_at: DateTime<Utc>,         // 更新时间
    pub deleted_at: Option<DateTime<Utc>>, // 删除时间（可选）
}

//////// END