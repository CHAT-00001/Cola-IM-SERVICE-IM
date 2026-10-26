// server/src/entity/user.rs -- ENTITY: User 用户信息实体
// 2026-01-05 10:19:20

////////

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

////////

/// [INFO] - 用户信息实体
#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct UserInfo {
    pub id: i64,
    pub user_nickname: String,
    pub avatar: String,
    pub level: String,
    pub usertype: String,
    pub vip_type: String,
    pub guard_type: String,
    pub liangname: String,
    pub car_id: String,
    pub car_swf: String,
    pub car_swftime: String,
    pub car_words: String,
}

//////// END
