// server/src/chat/port.rs -- Persistence and recipient-resolution ports
// 2026/10/7 21:46 Created.

pub use app_core::ports::message::{ContactRelationshipRepository, GroupMembershipRepository};
use async_trait::async_trait;

use crate::chat::model::{AckStage, PendingDelivery, PersistMessage, SenderEndpoint};

#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error("invalid message: {0}")]
    InvalidMessage(String),
    #[error("not authorized for conversation")]
    Unauthorized,
    #[error("message was not found or is not addressed to this account")]
    MessageNotFound,
    #[error("persistence failure: {0}")]
    Persistence(String),
}

#[async_trait]
pub trait MessageStore: Send + Sync {
    async fn persist_message_and_mailboxes(
        &self,
        message: &PersistMessage,
    ) -> Result<(), ChatError>;

    async fn acknowledge_device(
        &self,
        message_id: &str,
        account_id: u64,
        device_id: &str,
        stage: AckStage,
    ) -> Result<(), ChatError>;

    async fn pending_for_device(
        &self,
        account_id: u64,
        device_id: &str,
        limit: u32,
    ) -> Result<Vec<PendingDelivery>, ChatError>;

    async fn sender_endpoint(&self, message_id: &str) -> Result<SenderEndpoint, ChatError>;
}

#[async_trait]
pub trait DeviceConnectionRegistry: Send + Sync {
    async fn online_device_ids(&self, account_id: u64) -> Vec<String>;

    async fn enqueue_to_device(
        &self,
        account_id: u64,
        device_id: &str,
        message: &crate::models::message::MessageBody,
    ) -> Result<(), ChatError>;
}
