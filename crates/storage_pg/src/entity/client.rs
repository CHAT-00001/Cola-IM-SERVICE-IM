// storage_pg/src/entity/client.rs  -- 客户端数据库映射（带异常风控）
// 2026-03-24 17:06

////////

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

////////

/// # [ENTITY] - 客户端版本管理
/// * `pg schema`: `auth`
/// * `table name`: `client_version`
#[derive(Debug, FromRow)]
pub struct ClientVersionEntity {
    pub id: Uuid,                     // 客户端版本id
    pub status: i16,                  // 状态: 0. 过期 1. 正常 2. 异常 3. ban
    pub name: String,                 // 客户端名称
    pub desc: Option<String>,         // 描述
    pub version_code: i64,            // 当前本号（1.4.199 取1004199）
    pub os_type: i16,                 // 系统类型: 0. win 1. web 2. wasm 3. Android 4. iOS 5. MacOS
    pub min_os_version: Option<i64>,  // 最小os版本
    pub max_os_version: Option<i64>,  // 最大os版本
    pub signature: String,            // 签名
    pub secret_key: String,           // 秘钥
    pub is_public: i8,                // 是否公开: 0. 否 1. 是
    pub is_force_update: bool,        // 是否强制更新（当版本较旧时）
    pub gray_scale_test: Option<i16>, // 灰度测试百分比0-100
    pub operator_id: Option<i64>,     // 操作者id
    pub created_at: DateTime<Utc>,    // 创建时间
    pub updated_at: DateTime<Utc>,    // 更新时间
}

////////

/// # 客户端设备
/// * `pg schema`: `auth`
/// * `table name`: `client_device`
#[derive(Debug, FromRow)]
pub struct ClientDeviceEntity {
    pub id: Uuid,                           // 消息id
    pub status: i16,                        // 状态: 0. 失效 1. 正常 2. 异常 3. 禁用
    pub user_id: i64,                       // 归属用户uid
    pub device_type: i16,                   // 设备类型
    pub device_name: String,                // 设备名称
    pub os_version_code: i64,               // 操作系统版号
    pub device_unique_id: String,           // 设备唯一标识（UUID/IDFA）
    pub app_version_code: Option<i64>,      // 记录设备当前安装的应用版本，方便查 Bug
    pub payload: Option<serde_json::Value>, // 装载内容
    pub last_login_ip: Option<String>,      // 引用的消息（可选）
    pub last_active_at: DateTime<Utc>,      // 最后心跳（在线判断）
    pub is_online: i8,                      // 是否在线
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// # 客户端设备ip logger （风控使用）
/// * `pg schema`: `auth`
/// * `table name`: `client_device_ip_logger`
#[derive(Debug, FromRow)]
pub struct ClientDeviceIpLoggerEntity {
    pub id: Uuid,                            // 群id
    pub status: i16,                         // 状态: 0. 静默 1. 正常 2. 高频 3. 异常
    pub device_id: Uuid,                     // 设备id（公共聊天）
    pub dau: Option<i64>,                    // DUA 日活
    pub is_baned: i8,                        // 是否封禁: 0. 否 1. 是
    pub ban_end_time: Option<DateTime<Utc>>, // 封禁到期时间，为 None 表示未封禁
    pub ip_address: String,                  // IP地址
    pub country: Option<String>,             // 国家/地区
    pub province: Option<String>,            // 省份/州/
    pub city: Option<String>,                // 城市/郡/行政州
    pub district: Option<String>,            // 区/县
    pub at_message_id: Option<String>,       // 引用的消息（可选）
    pub msg_type: i16,                       // 消息类型
    pub first_request_at: DateTime<Utc>,     // 初次出现
    pub last_request_at: DateTime<Utc>,      // 最后出现
}

//////// END