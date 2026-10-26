// server/src/entity/gift.rs
// 2026-03-24 05:20

////////////////

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct GiftInfo {
    pub id: String,   // ID
    pub name: String, // 名称
    pub count: i32,   // 数量，默认1
    pub length: i32,  // 礼物动画时长
    pub icon: String, // 图标
}
