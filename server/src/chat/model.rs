// server/src/chat/model.rs -- Message persistence and acknowledgement models
// 2026/10/7 21:46 Created.

////////

use crate::models::message::MessageBody;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

////////

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationKind {
    Direct,
    Group,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticatedSession {
    pub account_id: u64,
    pub device_id: String,
    pub connection_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistMessage {
    pub message_id: String,
    pub conversation_id: String,
    pub conversation_kind: ConversationKind,
    pub sender_account_id: u64,
    pub sender_device_id: String,
    pub body: MessageBody,
    pub recipient_account_ids: Vec<u64>,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    Pending,
    Delivered,
    Read,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceAcknowledgement {
    pub state: DeliveryState,
    pub acknowledged_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxDocument {
    pub message_id: String,
    pub recipient_account_id: u64,
    pub device_acknowledgements: HashMap<String, DeviceAcknowledgement>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingDelivery {
    pub message_id: String,
    pub recipient_account_id: u64,
    pub message: MessageBody,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderEndpoint {
    pub account_id: u64,
    pub device_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AckStage {
    ServerPersisted,
    DeviceDelivered,
    DeviceRead,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageAcknowledgement {
    pub message_id: String,
    pub stage: AckStage,
    pub acknowledged_at: DateTime<Utc>,
    pub account_id: Option<u64>,
    pub device_id: Option<String>,
}

impl From<&crate::models::message::AckStage> for AckStage {
    fn from(stage: &crate::models::message::AckStage) -> Self {
        match stage {
            crate::models::message::AckStage::ServerPersisted => Self::ServerPersisted,
            crate::models::message::AckStage::DeviceDelivered => Self::DeviceDelivered,
            crate::models::message::AckStage::DeviceRead => Self::DeviceRead,
        }
    }
}

//////// END
