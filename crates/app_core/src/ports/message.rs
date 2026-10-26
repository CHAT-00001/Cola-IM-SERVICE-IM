// app_core/src/ports/message.rs
// 2026-03-24 04:23

////////////////

use crate::models::chat::{ChatListItem, ChatSessionListItem};
use crate::models::message::{Message, MessageQuery};
use crate::models::user::UserProfile;
use async_trait::async_trait;

#[async_trait]
pub trait MessageRepository: Send + Sync {
    /// 行为 1：保存一条消息（发送消息时调用）
    async fn save_message(&self, msg: &Message) -> Result<String, String>;

    /// 行为 2：按时间点增量获取某个房间的消息（进入聊天框时调用）
    async fn get_messages_after(&self, query: MessageQuery) -> Result<Vec<Message>, String>;

    /// 行为 3：获取用户的增量聊天列表（主界面刷新时调用）
    async fn get_chat_list_incremental(
        &self,
        user_id: &str,
        last_t: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<ChatListItem>, String>;
}

#[async_trait]
pub trait ContactRelationshipRepository: Send + Sync {
    /// 行为 1：判断发送方是否存在指向接收方的有效联系人关系
    async fn can_message(
        &self,
        sender_account_id: u64,
        recipient_account_id: u64,
    ) -> Result<bool, String>;

    /// 行为 2：把远端资料写入本地联系人档案（存在则更新）
    /// * `DESC`: `contact_profile.id` 与账号 ID 同值，从而与 `contact_list` 的关联天然成立
    async fn upsert_contact_profile(&self, profile: &UserProfile) -> Result<(), String>;

    /// 行为 3：补建/恢复 owner -> target 的有效联系人关系
    /// * `DESC`: `仅写入发起方单向关系，不替对方伪造通讯录条目`
    async fn ensure_contact_relation(
        &self,
        owner_account_id: u64,
        target_account_id: u64,
    ) -> Result<(), String>;
}

#[async_trait]
pub trait GroupMembershipRepository: Send + Sync {
    async fn list_member_account_ids(&self, group_id: &str) -> Result<Vec<u64>, String>;

    async fn is_member(&self, group_id: &str, account_id: u64) -> Result<bool, String>;
}

#[async_trait]
pub trait DirectChatSessionRepository: Send + Sync {
    async fn ensure_direct_session(
        &self,
        owner_account_id: u64,
        peer_account_id: u64,
    ) -> Result<crate::models::chat::ChatSessionView, String>;

    async fn update_last_message(
        &self,
        session_id: &str,
        message_id: &str,
        summary: &str,
        sent_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), String>;

    /// 行为 3：列出账号参与的所有会话（聊天列表）
    async fn list_sessions(&self, account_id: u64)
        -> Result<Vec<ChatSessionListItem>, String>;

    /// 行为 4：按会话 ID 与成员账号反查单聊对端账号
    /// 用于 `/message/send` 只带 session_id、未带 recipient_uid 时兜底。
    async fn peer_account_id_in_session(
        &self,
        session_id: &str,
        account_id: u64,
    ) -> Result<Option<u64>, String>;
}

#[async_trait]
pub trait DirectConversationBlocklist: Send + Sync {
    async fn is_blocked(
        &self,
        blocker_account_id: u64,
        blocked_account_id: u64,
    ) -> Result<bool, String>;
}
