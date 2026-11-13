// app_core/src/models/conn.rs -- CORE - MODELS - 连接
// 2026/11/02 07:10

////////

use serde::{Deserialize, Serialize};

////////

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectBody {
    pub conn: ConnInfo,
}

/// # [CONN] - 连接信息
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnInfo {
    pub uid: String,
    pub room_id: String,
    pub nickname: String,
    pub stream: String,
    pub equipment: String,
    pub token: String,
}

//////// END