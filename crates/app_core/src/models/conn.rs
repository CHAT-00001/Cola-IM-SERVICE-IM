use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectBody {
    pub conn: ConnInfo,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnInfo {
    pub uid: String,
    pub room_id: String,
    pub nickname: String,
    pub stream: String,
    pub equipment: String,
    pub token: String,
}
