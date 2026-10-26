// server/src/chat/service.rs -- Direct/group messaging and device acknowledgements
// 2026/10/11 17:17 Created.

////////

use crate::chat::model::{
    AckStage, AuthenticatedSession, ConversationKind, MessageAcknowledgement, PendingDelivery,
    PersistMessage,
};
use crate::chat::port::{
    ChatError, DeviceConnectionRegistry, GroupMembershipRepository, MessageStore,
};
use crate::models::message::{MessageBody, MsgChannel, Payload, Status};
use app_core::ports::message::{
    ContactRelationshipRepository, DirectChatSessionRepository, DirectConversationBlocklist,
};
use chrono::Utc;

////////

#[derive(Clone)]
pub struct DirectMessageService<S, C, D, R, B> {
    store: S,
    contacts: C,
    devices: D,
    sessions: R,
    blocklist: B,
}
#[derive(Clone)]
pub struct GroupMessageService<S, G, D> {
    store: S,
    groups: G,
    devices: D,
}
#[derive(Clone)]
pub struct AcknowledgementService<S> {
    store: S,
}

impl<S, C, D, R, B> DirectMessageService<S, C, D, R, B>
where
    S: MessageStore,
    C: ContactRelationshipRepository,
    D: DeviceConnectionRegistry,
    R: DirectChatSessionRepository,
    B: DirectConversationBlocklist,
{
    /// # [HANDLER] - new
    /// * `DESC`: `创建单聊消息服务`
    pub fn new(store: S, contacts: C, devices: D, sessions: R, blocklist: B) -> Self {
        Self {
            store,
            contacts,
            devices,
            sessions,
            blocklist,
        }
    }

    /// # [HANDLER] - send
    /// * `DESC`: `校验权限、持久化并投递单聊消息`
    pub async fn send(
        &self,
        session: &AuthenticatedSession,
        recipient: u64,
        message: MessageBody,
    ) -> Result<MessageAcknowledgement, ChatError> {
        validate_message(&message)?;
        if recipient == session.account_id {
            return Err(ChatError::InvalidMessage(
                "recipient must differ from sender".to_owned(),
            ));
        }
        for (blocker, blocked) in [
            (session.account_id, recipient),
            (recipient, session.account_id),
        ] {
            if self
                .blocklist
                .is_blocked(blocker, blocked)
                .await
                .map_err(|e| ChatError::Persistence(format!("blocklist: {e}")))?
            {
                return Err(ChatError::Unauthorized);
            }
        }
        if !self
            .contacts
            .can_message(session.account_id, recipient)
            .await
            .map_err(|e| ChatError::Persistence(format!("contact repository: {e}")))?
        {
            return Err(ChatError::Unauthorized);
        }
        let chat = self
            .sessions
            .ensure_direct_session(session.account_id, recipient)
            .await
            .map_err(|e| ChatError::Persistence(format!("session repository: {e}")))?;
        if chat.status != 1 || chat.member_state != 1 {
            return Err(ChatError::Unauthorized);
        }
        let persisted = make_persisted_message(
            session,
            message,
            ConversationKind::Direct,
            chat.session_id,
            vec![recipient],
        );
        self.store.persist_message_and_mailboxes(&persisted).await?;
        self.sessions
            .update_last_message(
                &persisted.conversation_id,
                &persisted.message_id,
                &message_summary(&persisted.body),
                persisted.created_at,
            )
            .await
            .map_err(|e| ChatError::Persistence(format!("session repository: {e}")))?;
        for device in self.devices.online_device_ids(session.account_id).await {
            if device != session.device_id {
                let _ = self
                    .devices
                    .enqueue_to_device(session.account_id, &device, &persisted.body)
                    .await;
            }
        }
        for device in self.devices.online_device_ids(recipient).await {
            let _ = self
                .devices
                .enqueue_to_device(recipient, &device, &persisted.body)
                .await;
        }

        ////////

        // 💡 - LOGGER - 日志
        println!(
            "[✅️ SEND MSG] - a: {} > b: {} 发送了一条消息",
            session.account_id, recipient
        );

        ////////

        // ✅️ - Ok
        Ok(server_persisted_ack(&persisted.message_id))
    }

    /// # [HANDLER] - online_recipient_devices
    /// * `DESC`: `获取接收账号在线设备列表`
    pub async fn online_recipient_devices(&self, recipient: u64) -> Vec<String> {
        self.devices.online_device_ids(recipient).await
    }

    /// # [HANDLER] - resolve_session_peer
    /// * `DESC`: `按会话 ID 反查单聊对端，供只带 session_id 的发送请求兜底`
    pub async fn resolve_session_peer(
        &self,
        session_id: &str,
        account_id: u64,
    ) -> Result<Option<u64>, ChatError> {
        if session_id.trim().is_empty() {
            return Ok(None);
        }
        self.sessions
            .peer_account_id_in_session(session_id, account_id)
            .await
            .map_err(|error| ChatError::Persistence(format!("session repository: {error}")))
    }
}

////////

impl<S, G, D> GroupMessageService<S, G, D>
where
    S: MessageStore,
    G: GroupMembershipRepository,
    D: DeviceConnectionRegistry,
{
    /// # [HANDLER] - new
    /// * `DESC`: `创建群聊消息服务`
    pub fn new(store: S, groups: G, devices: D) -> Self {
        Self {
            store,
            groups,
            devices,
        }
    }

    /// # [HANDLER] - send
    /// * `DESC`: `校验群成员并投递群聊消息`
    pub async fn send(
        &self,
        session: &AuthenticatedSession,
        group: &str,
        message: MessageBody,
    ) -> Result<MessageAcknowledgement, ChatError> {
        validate_message(&message)?;
        if group.trim().is_empty() {
            return Err(ChatError::InvalidMessage("group id is empty".to_owned()));
        }
        if !self
            .groups
            .is_member(group, session.account_id)
            .await
            .map_err(|e| ChatError::Persistence(format!("group repository: {e}")))?
        {
            return Err(ChatError::Unauthorized);
        }
        let mut recipients = self
            .groups
            .list_member_account_ids(group)
            .await
            .map_err(|e| ChatError::Persistence(format!("group repository: {e}")))?;
        recipients.sort_unstable();
        recipients.dedup();
        recipients.retain(|id| *id != session.account_id);
        let persisted = make_persisted_message(
            session,
            message,
            ConversationKind::Group,
            group.to_owned(),
            recipients,
        );
        self.store.persist_message_and_mailboxes(&persisted).await?;
        for account in std::iter::once(session.account_id)
            .chain(persisted.recipient_account_ids.iter().copied())
        {
            for device in self.devices.online_device_ids(account).await {
                if account != session.account_id || device != session.device_id {
                    let _ = self
                        .devices
                        .enqueue_to_device(account, &device, &persisted.body)
                        .await;
                }
            }
        }
        Ok(server_persisted_ack(&persisted.message_id))
    }

    /// # [HANDLER] - online_devices_for_group
    /// * `DESC`: `查询群组成员在线设备`
    pub async fn online_devices_for_group(
        &self,
        group: &str,
    ) -> Result<Vec<(u64, Vec<String>)>, ChatError> {
        let mut ids = self
            .groups
            .list_member_account_ids(group)
            .await
            .map_err(|e| ChatError::Persistence(format!("group repository: {e}")))?;
        ids.sort_unstable();
        ids.dedup();
        let mut result = Vec::new();
        for id in ids {
            let devices = self.devices.online_device_ids(id).await;
            if !devices.is_empty() {
                result.push((id, devices));
            }
        }
        Ok(result)
    }
}

////////
fn validate_message(message: &MessageBody) -> Result<(), ChatError> {
    if message.send_id.trim().is_empty() {
        return Err(ChatError::InvalidMessage("send_id is empty".to_owned()));
    }
    match &message.payload {
        Payload::Content { text } if text.trim().is_empty() => Err(ChatError::InvalidMessage(
            "message content is empty".to_owned(),
        )),
        Payload::System { message } if message.trim().is_empty() => Err(ChatError::InvalidMessage(
            "system message is empty".to_owned(),
        )),
        Payload::Barrage(value) if value.text.trim().is_empty() => Err(ChatError::InvalidMessage(
            "barrage content is empty".to_owned(),
        )),
        Payload::Gift(value) if value.name.trim().is_empty() => {
            Err(ChatError::InvalidMessage("gift name is empty".to_owned()))
        }
        Payload::Media(value)
            if value
                .url
                .as_ref()
                .map(|url| url.trim().is_empty())
                .unwrap_or(true) =>
        {
            Err(ChatError::InvalidMessage("media URL is empty".to_owned()))
        }
        _ => Ok(()),
    }
}

/// # [HANDLER] - message_summary
/// * `DESC`: `从消息载荷生成用于聊天列表展示的摘要`
fn message_summary(message: &MessageBody) -> String {
    match &message.payload {
        Payload::Content { text } => text.clone(),
        Payload::System { message } => message.clone(),
        Payload::Barrage(value) => value.text.clone(),
        Payload::Gift(value) => value.name.clone(),
        Payload::Media(value) => value
            .name
            .clone()
            .or_else(|| value.url.clone())
            .unwrap_or_else(|| "[media]".to_owned()),
    }
}

/// # [HANDLER] - make_persisted_message
/// * `DESC`: `构造持久化会话消息并覆盖客户端伪造的发送者身份`
fn make_persisted_message(
    session: &AuthenticatedSession,
    mut body: MessageBody,
    kind: ConversationKind,
    conversation_id: String,
    recipients: Vec<u64>,
) -> PersistMessage {
    let now = Utc::now();
    body.status = Status::Syncing;
    body.client_id = session.device_id.clone();
    body.uid = session.account_id;
    body.room_id = conversation_id.clone();
    body.channel = Some(match kind {
        ConversationKind::Direct => MsgChannel::single,
        ConversationKind::Group => MsgChannel::group,
    });
    body.server_sync_at = Some(now);
    body.device_id = Some(session.device_id.clone());
    PersistMessage {
        message_id: body.send_id.clone(),
        conversation_id,
        conversation_kind: kind,
        sender_account_id: session.account_id,
        sender_device_id: session.device_id.clone(),
        body,
        recipient_account_ids: recipients,
        created_at: now,
    }
}

/// # [HANDLER] - server_persisted_ack
/// * `DESC`: `构造服务端已持久化确认`
fn server_persisted_ack(message_id: &str) -> MessageAcknowledgement {
    MessageAcknowledgement {
        message_id: message_id.to_owned(),
        stage: AckStage::ServerPersisted,
        acknowledged_at: Utc::now(),
        account_id: None,
        device_id: None,
    }
}

//////// END

impl<S: MessageStore> AcknowledgementService<S> {
    /// # [HANDLER] - new
    /// * `DESC`: `创建 ACK 服务`
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// # [HANDLER] - acknowledge_device
    /// * `DESC`: `持久化指定设备 ACK`
    pub async fn acknowledge_device(
        &self,
        session: &AuthenticatedSession,
        id: &str,
        stage: AckStage,
    ) -> Result<MessageAcknowledgement, ChatError> {
        if id.trim().is_empty() || session.device_id.trim().is_empty() {
            return Err(ChatError::InvalidMessage(
                "message or device id is empty".to_owned(),
            ));
        }
        if stage == AckStage::ServerPersisted {
            return Err(ChatError::InvalidMessage(
                "server persistence ACK is generated after saving the message".to_owned(),
            ));
        }
        self.store
            .acknowledge_device(id, session.account_id, &session.device_id, stage)
            .await?;
        Ok(MessageAcknowledgement {
            message_id: id.to_owned(),
            stage,
            acknowledged_at: Utc::now(),
            account_id: Some(session.account_id),
            device_id: Some(session.device_id.clone()),
        })
    }

    /// # [HANDLER] - acknowledgement_recipient
    /// * `DESC`: `查询消息原始发送者账号和设备`
    pub async fn acknowledgement_recipient(
        &self,
        id: &str,
    ) -> Result<crate::chat::model::SenderEndpoint, ChatError> {
        self.store.sender_endpoint(id).await
    }

    /// # [HANDLER] - pending_for_device
    /// * `DESC`: `查询设备未确认消息`
    pub async fn pending_for_device(
        &self,
        session: &AuthenticatedSession,
        limit: u32,
    ) -> Result<Vec<PendingDelivery>, ChatError> {
        if session.device_id.trim().is_empty() {
            return Err(ChatError::InvalidMessage("device id is empty".to_owned()));
        }
        self.store
            .pending_for_device(session.account_id, &session.device_id, limit.clamp(1, 100))
            .await
    }
}

////////

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use super::DirectMessageService;
    use crate::chat::model::{AckStage, PendingDelivery, PersistMessage, SenderEndpoint};
    use crate::chat::port::{ChatError, DeviceConnectionRegistry, MessageStore};
    use crate::models::message::MessageBody;
    use app_core::models::chat::{ChatSessionListItem, ChatSessionView};
    use app_core::ports::message::{
        ContactRelationshipRepository, DirectChatSessionRepository, DirectConversationBlocklist,
    };

    #[derive(Clone)]
    struct NoopStore;

    #[async_trait]
    impl MessageStore for NoopStore {
        async fn persist_message_and_mailboxes(
            &self,
            _message: &PersistMessage,
        ) -> Result<(), ChatError> {
            Ok(())
        }

        async fn acknowledge_device(
            &self,
            _message_id: &str,
            _account_id: u64,
            _device_id: &str,
            _stage: AckStage,
        ) -> Result<(), ChatError> {
            Ok(())
        }

        async fn pending_for_device(
            &self,
            _account_id: u64,
            _device_id: &str,
            _limit: u32,
        ) -> Result<Vec<PendingDelivery>, ChatError> {
            Ok(Vec::new())
        }

        async fn sender_endpoint(&self, _message_id: &str) -> Result<SenderEndpoint, ChatError> {
            Err(ChatError::MessageNotFound)
        }
    }

    #[derive(Clone)]
    struct NoopDevices;

    #[async_trait]
    impl DeviceConnectionRegistry for NoopDevices {
        async fn online_device_ids(&self, _account_id: u64) -> Vec<String> {
            Vec::new()
        }

        async fn enqueue_to_device(
            &self,
            _account_id: u64,
            _device_id: &str,
            _message: &MessageBody,
        ) -> Result<(), ChatError> {
            Ok(())
        }
    }

    #[derive(Clone)]
    struct AllowAllContacts;

    #[async_trait]
    impl ContactRelationshipRepository for AllowAllContacts {
        async fn can_message(&self, _: u64, _: u64) -> Result<bool, String> {
            Ok(true)
        }

        async fn upsert_contact_profile(
            &self,
            _: &app_core::models::user::UserProfile,
        ) -> Result<(), String> {
            Ok(())
        }

        async fn ensure_contact_relation(&self, _: u64, _: u64) -> Result<(), String> {
            Ok(())
        }
    }

    #[derive(Clone)]
    struct NoBlocklist;

    #[async_trait]
    impl DirectConversationBlocklist for NoBlocklist {
        async fn is_blocked(&self, _: u64, _: u64) -> Result<bool, String> {
            Ok(false)
        }
    }

    #[derive(Clone, Default)]
    struct FakeSessions {
        peers: Arc<Mutex<Vec<(String, u64, u64)>>>,
    }

    #[async_trait]
    impl DirectChatSessionRepository for FakeSessions {
        async fn ensure_direct_session(&self, _: u64, _: u64) -> Result<ChatSessionView, String> {
            Err("ensure_direct_session is not used by resolve_session_peer".to_owned())
        }

        async fn update_last_message(
            &self,
            _: &str,
            _: &str,
            _: &str,
            _: chrono::DateTime<chrono::Utc>,
        ) -> Result<(), String> {
            Ok(())
        }

        async fn list_sessions(&self, _: u64) -> Result<Vec<ChatSessionListItem>, String> {
            Ok(Vec::new())
        }

        async fn peer_account_id_in_session(
            &self,
            session_id: &str,
            account_id: u64,
        ) -> Result<Option<u64>, String> {
            Ok(self
                .peers
                .lock()
                .unwrap()
                .iter()
                .find(|(id, member, _)| id == session_id && *member == account_id)
                .map(|(_, _, peer)| *peer))
        }
    }

    /// # [HANDLER] - resolve_session_peer_returns_session_counterpart
    /// * `DESC`: `验证单聊只带 session_id 时能反查出对端账号`
    #[tokio::test]
    async fn resolve_session_peer_returns_session_counterpart() {
        let sessions = FakeSessions::default();
        sessions
            .peers
            .lock()
            .unwrap()
            .push(("session-1".to_owned(), 10, 20));
        let service = DirectMessageService::new(
            NoopStore,
            AllowAllContacts,
            NoopDevices,
            sessions,
            NoBlocklist,
        );

        assert_eq!(
            service.resolve_session_peer("session-1", 10).await.unwrap(),
            Some(20)
        );
    }

    /// # [HANDLER] - resolve_session_peer_rejects_unknown_session
    /// * `DESC`: `验证会话缺失或不属于当前账号时不返回对端，交由 recipient_uid 校验报错`
    #[tokio::test]
    async fn resolve_session_peer_rejects_unknown_session() {
        let sessions = FakeSessions::default();
        sessions
            .peers
            .lock()
            .unwrap()
            .push(("session-1".to_owned(), 10, 20));
        let service = DirectMessageService::new(
            NoopStore,
            AllowAllContacts,
            NoopDevices,
            sessions,
            NoBlocklist,
        );

        assert_eq!(service.resolve_session_peer("session-2", 10).await.unwrap(), None);
        assert_eq!(service.resolve_session_peer("session-1", 30).await.unwrap(), None);
        assert_eq!(service.resolve_session_peer("", 10).await.unwrap(), None);
    }
}

//////// END
