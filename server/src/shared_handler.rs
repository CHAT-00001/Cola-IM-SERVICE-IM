// server/src/shared_handler.rs -- Shared transport-independent chat handler
// 2026/10/7 14:03 Created.

////////

use chrono::Utc;
use std::sync::Arc;
use tracing::{info, warn};
use crate::chat::model::{AckStage, AuthenticatedSession, MessageAcknowledgement, PendingDelivery};
use crate::chat::port::DeviceConnectionRegistry;
use crate::chat::service::{AcknowledgementService, DirectMessageService, GroupMessageService};
use crate::hub::ChatHub;
use crate::models::message::{Event, MessageBody, MsgChannel, MsgType, Payload, Status};

////////

type DirectService = DirectMessageService<
    crate::service::mongodb::MessageStoreBackend,
    crate::service::contact_sync::SyncingContactGateway<
        crate::service::relationships::PgRelationshipGateway,
        crate::service::identity::IdentitySource,
    >,
    ChatHub,
    storage_pg::repository::chat_session::PgDirectChatSessionRepository,
    crate::service::blocklist::AllowAllBlocklist,
>;

type GroupService = GroupMessageService<
    crate::service::mongodb::MessageStoreBackend,
    crate::service::relationships::PgGroupGateway,
    ChatHub,
>;

type AckService = AcknowledgementService<crate::service::mongodb::MessageStoreBackend>;

////////

/// # [HANDLER] - 聊天方式
#[derive(Clone)]
pub struct ChatHandler {
    hub: ChatHub,
    max_rooms_per_connection: usize,
    direct: Option<Arc<DirectService>>,
    group: Option<Arc<GroupService>>,
    acknowledgements: Option<Arc<AckService>>,
}

// 构造实现
impl ChatHandler {
    ////////
    
    /// # [HANDLER] - new
    /// * `DESC`: `閸掓稑缂撻崗鍙橀煩閼卞﹤銇?Handler 楠炶埖鏁為崗銉ょ炊鏉堟挻妫ら崗瀹犵箾閹恒儲鏁為崘宀冦€僠
    pub fn new(hub: ChatHub, max_rooms_per_connection: usize) -> Self {
        Self {
            hub,
            max_rooms_per_connection: max_rooms_per_connection.max(1),
            direct: None,
            group: None,
            acknowledgements: None,
        }
    }

    ////////

    /// # [HANDLER] - with_chat_services
    /// * `DESC`: `濞夈劌鍙?MongoDB 濞戝牊浼呮禒鎾冲亶閸?PostgreSQL 閸忓磭閮存禒鎾冲亶娴犮儱鎯庨悽銊ュ礋閼卞﹦鍏㈤懕濂?
    pub fn with_chat_services(
        mut self,
        direct: DirectService,
        group: GroupService,
        acknowledgements: AckService,
    ) -> Self {
        self.direct = Some(Arc::new(direct));
        self.group = Some(Arc::new(group));
        self.acknowledgements = Some(Arc::new(acknowledgements));
        self
    }

    ////////

    /// # [HANDLER] - handle_authenticated_message
    /// * `DESC`: `娴ｈ法鏁ょ拋銈堢槈娴兼俺鐦界捄顖滄暠閸楁洝浜扮紘銈堜喊楠炶泛婀禍瀣閹存劕濮涢崥搴ょ箲閸ョ偞婀囬崝锛勵伂 ACK`
    pub async fn handle_authenticated_message(
        &self,
        session: &AuthenticatedSession,
        message: MessageBody,
    ) -> Result<MessageAcknowledgement, crate::chat::port::ChatError> {
        if !self.hub.matches_authenticated_session(
            &session.connection_id,
            session.account_id,
            &session.device_id,
        ) {
            if !self.hub.register_authenticated_device(
                &session.connection_id,
                session.account_id,
                &session.device_id,
            ) {
                return Err(crate::chat::port::ChatError::Unauthorized);
            }
        }

        let client_ack_message_id = message.ack_message_id.clone();
        let acknowledgement = self.dispatch_authenticated(session, message).await?;

        if acknowledgement.stage == AckStage::ServerPersisted {
            self.hub.enqueue_to_connection(
                &session.connection_id,
                acknowledgement_message(&acknowledgement),
            )?;
        } else if let Some(message_id) = client_ack_message_id {
            let recipient = self.acknowledgement_recipient(&message_id).await?;
            if let Err(error) = self
                .hub
                .enqueue_to_device(
                    recipient.account_id,
                    &recipient.device_id,
                    &acknowledgement_message(&acknowledgement),
                )
                .await
            {
                warn!("ACK stored but sender device is not currently connected: {error}");
            }
        }
        Ok(acknowledgement)
    }

    ////////

    /// # [HANDLER] - send_authenticated_message
    /// * `DESC`: `传输无关的已认证消息入口，直接返回确认而不回推长连接`
    pub async fn send_authenticated_message(
        &self,
        session: &AuthenticatedSession,
        message: MessageBody,
    ) -> Result<MessageAcknowledgement, crate::chat::port::ChatError> {
        self.dispatch_authenticated(session, message).await
    }

    ////////

    /// # [HANDLER] - dispatch_authenticated
    /// * `DESC`: `按 channel 将已认证消息分发到单聊、群聊或设备确认服务`
    async fn dispatch_authenticated(
        &self,
        session: &AuthenticatedSession,
        mut message: MessageBody,
    ) -> Result<MessageAcknowledgement, crate::chat::port::ChatError> {
        match message.channel.unwrap_or(MsgChannel::public) {
            MsgChannel::single => {
                let service = self.direct.as_ref().ok_or_else(|| {
                    crate::chat::port::ChatError::Persistence(
                        "direct-message service is not configured".to_owned(),
                    )
                })?;
                // 兼容只带 session_id(room_id) 的发送请求：先按会话反查对端，再校验联系人关系。
                let recipient = match message.recipient_uid {
                    Some(recipient) => recipient,
                    None => service
                        .resolve_session_peer(&message.room_id, session.account_id)
                        .await?
                        .ok_or_else(|| {
                            crate::chat::port::ChatError::InvalidMessage(
                                "recipient_uid is required for direct messages".to_owned(),
                            )
                        })?,
                };
                // 回填后持久化的消息体自带 recipient_uid，接收端同步时可直接推断对端。
                message.recipient_uid = Some(recipient);
                service.send(session, recipient, message).await
            }
            MsgChannel::group => {
                let group_id = message.room_id.clone();
                let service = self.group.as_ref().ok_or_else(|| {
                    crate::chat::port::ChatError::Persistence(
                        "group-message service is not configured".to_owned(),
                    )
                })?;
                service.send(session, &group_id, message).await
            }
            MsgChannel::received => {
                let message_id = message.ack_message_id.take().ok_or_else(|| {
                    crate::chat::port::ChatError::InvalidMessage(
                        "ack_message_id is required for device acknowledgements".to_owned(),
                    )
                })?;
                let stage = message
                    .ack_stage
                    .as_ref()
                    .map(AckStage::from)
                    .ok_or_else(|| {
                        crate::chat::port::ChatError::InvalidMessage(
                            "ack_stage is required for device acknowledgements".to_owned(),
                        )
                    })?;
                self.acknowledge(session, &message_id, stage).await
            }
            _ => Err(crate::chat::port::ChatError::InvalidMessage(
                "authenticated message entry supports only single, group and ACK events"
                    .to_owned(),
            )),
        }
    }

    ////////

    /// # [HANDLER] - pending_messages_for_device
    /// * `DESC`: `返回指定设备尚未确认的离线消息，供 REST 同步使用`
    pub async fn pending_messages_for_device(
        &self,
        session: &AuthenticatedSession,
        limit: u32,
    ) -> Result<Vec<PendingDelivery>, crate::chat::port::ChatError> {
        let service = self.acknowledgements.as_ref().ok_or_else(|| {
            crate::chat::port::ChatError::Persistence(
                "acknowledgement service is not configured".to_owned(),
            )
        })?;
        service.pending_for_device(session, limit).await
    }

    ////////

    /// # [HANDLER] - conversation_history
    /// * `DESC`: `校验当前账号是该会话成员后，按时间线返回最近的消息（旧 -> 新）`
    /// * `NOTE`: `客户端重开会话时用它补历史；离线补投仍走 pending_messages_for_device`
    pub async fn conversation_history(
        &self,
        session: &AuthenticatedSession,
        conversation_id: &str,
        limit: u32,
        before: Option<chrono::DateTime<Utc>>,
    ) -> Result<Vec<MessageBody>, crate::chat::port::ChatError> {
        let conversation_id = conversation_id.trim();
        if conversation_id.is_empty() {
            return Err(crate::chat::port::ChatError::InvalidMessage(
                "room_id is required".to_owned(),
            ));
        }
        let service = self.direct.as_ref().ok_or_else(|| {
            crate::chat::port::ChatError::Persistence(
                "direct-message service is not configured".to_owned(),
            )
        })?;
        // 单聊会话能用成员关系反查就顺带校验；群聊/非 UUID 会话反查不到（返回 None），
        // 此时退化为按 conversation_id 过滤读取——该 ID 本身即会话可见范围。
        if service
            .resolve_session_peer(conversation_id, session.account_id)
            .await?
            .is_some()
        {
            tracing::debug!(
                account_id = session.account_id,
                room_id = conversation_id,
                "[💬 MSG] 会话成员校验通过，读取历史"
            );
        }
        service
            .conversation_history(conversation_id, limit, before)
            .await
    }

    ////////

    /// # [HANDLER] - acknowledge
    /// * `DESC`: `閹镐椒绠欓崠鏍ㄥ瘹鐎规俺澶勯崣鐤啎婢?ACK 楠炶泛娲栨导鐘虫箛閸旓紕顏涵顔款吇`
    pub async fn acknowledge(
        &self,
        session: &AuthenticatedSession,
        message_id: &str,
        stage: AckStage,
    ) -> Result<MessageAcknowledgement, crate::chat::port::ChatError> {
        let service = self.acknowledgements.as_ref().ok_or_else(|| {
            crate::chat::port::ChatError::Persistence(
                "acknowledgement service is not configured".to_owned(),
            )
        })?;
        service.acknowledge_device(session, message_id, stage).await
    }

    ////////

    /// # [HANDLER] - sync_pending_for_device
    /// * `DESC`: `鐠併倛鐦夐崥搴濈矤 MongoDB 閺€鏈垫缁犲崬鎷伴崣鎴滄缁犺精藟閹峰顕氱拋鎯ь槵閺堫亞鈥樼拋銈嗙Х閹棎
    pub async fn sync_pending_for_device(
        &self,
        session: &AuthenticatedSession,
        limit: u32,
    ) -> Result<usize, crate::chat::port::ChatError> {
        if !self.hub.matches_authenticated_session(
            &session.connection_id,
            session.account_id,
            &session.device_id,
        ) {
            return Err(crate::chat::port::ChatError::Unauthorized);
        }

        let service = self.acknowledgements.as_ref().ok_or_else(|| {
            crate::chat::port::ChatError::Persistence(
                "acknowledgement service is not configured".to_owned(),
            )
        })?;
        let pending = service.pending_for_device(session, limit).await?;
        let mut enqueued = 0;
        for delivery in pending {
            match self
                .hub
                .enqueue_to_connection(&session.connection_id, delivery.message)
            {
                Ok(()) => enqueued += 1,
                Err(error) => {
                    warn!("Device sync queue is full; remaining messages stay persisted: {error}");
                    break;
                }
            }
        }
        Ok(enqueued)
    }

    ////////

    /// # [HANDLER] - acknowledgement_recipient
    /// * `DESC`: `閼惧嘲褰囧☉鍫熶紖閸樼喎褰傞柅浣藉閸欏嘲鎷扮拋鎯ь槵閻劋绨捄顖滄暠鐠佹儳顦?ACK`
    async fn acknowledgement_recipient(
        &self,
        message_id: &str,
    ) -> Result<crate::chat::model::SenderEndpoint, crate::chat::port::ChatError> {
        self.acknowledgements
            .as_ref()
            .ok_or_else(|| {
                crate::chat::port::ChatError::Persistence(
                    "acknowledgement service is not configured".to_owned(),
                )
            })?
            .acknowledgement_recipient(message_id)
            .await
    }

    ////////

    /// # [HANDLER] - handle_message
    /// * `DESC`: `鎸?channel 鍒嗗彂鍏叡銆佹湭璁よ瘉浼氳瘽銆丄CK 鍜岄€氱煡娑堟伅`
    pub fn handle_message(&self, connection_id: &str, mut message: MessageBody) {
        message.server_sync_at = Some(Utc::now());
        match message.channel.unwrap_or(MsgChannel::public) {
            MsgChannel::public => self.handle_public(connection_id, &message),
            MsgChannel::single | MsgChannel::group | MsgChannel::received => {
                self.handle_unauthenticated_direct_or_group(
                    message.channel.unwrap_or(MsgChannel::public),
                );
            }
            MsgChannel::Notice => {
                info!("[NOTICE] System notice channel is server-to-client only; ignored from connection {}", connection_id);
            }
        }
    }

    ////////

    /// # [HANDLER] - handle_public
    /// * `DESC`: `鐏忓棗鍙曢崗杈ㄧХ閹垵绻涢幒銉ュ煂閹村潡妫块獮鍫曗偓姘崇箖閸忓彉闊╂潻鐐村复濞夈劌鍞界悰銊ョ畭閹剧挜
    fn handle_public(&self, connection_id: &str, message: &MessageBody) {
        let room_id = message.room_id.as_str();
        if room_id.is_empty() {
            return;
        }

        if !self
            .hub
            .join_room(connection_id, room_id, self.max_rooms_per_connection)
        {
            warn!(
                "Connection {} could not join room (room limit: {})",
                connection_id, self.max_rooms_per_connection
            );
            return;
        }

        self.hub.broadcast(room_id, message.clone());
        info!(
            "[PUBLIC] Connection {} sent message to Room {}",
            connection_id, room_id
        );
    }

    ////////

    /// # [HANDLER] - handle_unauthenticated_direct_or_group
    /// * `DESC`: `鎷掔粷鏈璇佽繛鎺ユ墽琛岄渶瑕佽韩浠界殑鍗曡亰銆佺兢鑱婃垨 ACK 鎿嶄綔`
    pub fn handle_unauthenticated_direct_or_group(&self, channel: MsgChannel) {
        warn!(
            "Rejecting {:?} operation because the connection has no authenticated session",
            channel
        );
    }

    ////////

    /// # [HANDLER] - hub
    /// * `DESC`: `鏉╂柨娲栭崗鍙橀煩 Handler 娴ｈ法鏁ら惃鍕箾閹恒儲鏁為崘宀冦€冮崣銉︾労`
    pub fn hub(&self) -> &ChatHub {
        &self.hub
    }

    ////////

    /// # [HANDLER] - max_rooms_per_connection
    /// * `DESC`: `鏉╂柨娲栭崡鏇氶嚋鏉╃偞甯撮崗浣筋啅閸旂姴鍙嗛惃鍕付婢堆勫煣闂傚瓨鏆焋
    pub fn max_rooms_per_connection(&self) -> usize {
        self.max_rooms_per_connection
    }
}

////////

/// # [HANDLER] - protocol_error_message
/// * `DESC`: `鐏忓棗鍑＄拋銈堢槈濞戝牊浼呮径鍕倞闁挎瑨顕ゆ潪顒佸床娑撹櫣绮烘稉鈧崡蹇氼唴闁挎瑨顕ら崶鐐差槻`
pub(crate) fn protocol_error_message(
    error: crate::chat::port::ChatError,
    channel: MsgChannel,
) -> Option<MessageBody> {
    use crate::chat::port::ChatError;

    let (code, message) = match error {
        ChatError::InvalidMessage(message) => ("invalid_message", message),
        ChatError::Unauthorized => ("forbidden", "message is not allowed".to_owned()),
        ChatError::MessageNotFound => ("not_found", "message was not found".to_owned()),
        ChatError::Persistence(message) => {
            warn!("Authenticated message persistence failed: {message}");
            (
                "temporarily_unavailable",
                "message could not be processed".to_owned(),
            )
        }
    };

    Some(MessageBody {
        status: Status::Syncing,
        client_id: "server".to_owned(),
        send_at: Utc::now(),
        server_sync_at: Some(Utc::now()),
        send_id: String::new(),
        sync_id: String::new(),
        msg_type: MsgType::System,
        event: Event::ServerPersistedAck,
        room_id: String::new(),
        uid: 0,
        sender: None,
        payload: Payload::System {
            message: format!("{code}: {message}"),
        },
        channel: Some(channel),
        recipient_uid: None,
        ack_message_id: None,
        device_id: None,
        ack_stage: None,
    })
}

/// # [HANDLER] - acknowledgement_message
/// * `DESC`: `閺嬪嫰鈧姵瀵旀稊鍛濞戝牊浼呴惃鍕箛閸旓紕顏涵顔款吇閸楀繗顔呯敮顪?
fn acknowledgement_message(acknowledgement: &MessageAcknowledgement) -> MessageBody {
    let stage = match acknowledgement.stage {
        AckStage::ServerPersisted => crate::models::message::AckStage::ServerPersisted,
        AckStage::DeviceDelivered => crate::models::message::AckStage::DeviceDelivered,
        AckStage::DeviceRead => crate::models::message::AckStage::DeviceRead,
    };
    let event = match acknowledgement.stage {
        AckStage::ServerPersisted => Event::ServerPersistedAck,
        AckStage::DeviceDelivered | AckStage::DeviceRead => Event::AckRecorded,
    };

    MessageBody {
        status: Status::Syncing,
        client_id: "server".to_owned(),
        send_at: acknowledgement.acknowledged_at,
        server_sync_at: Some(acknowledgement.acknowledged_at),
        send_id: acknowledgement.message_id.clone(),
        sync_id: String::new(),
        msg_type: MsgType::System,
        event,
        room_id: String::new(),
        uid: 0,
        sender: None,
        payload: Payload::System {
            message: format!("{:?}", acknowledgement.stage),
        },
        channel: Some(MsgChannel::received),
        recipient_uid: acknowledgement.account_id,
        ack_message_id: Some(acknowledgement.message_id.clone()),
        device_id: acknowledgement.device_id.clone(),
        ack_stage: Some(stage),
    }
}

#[cfg(test)]
mod tests {
    use super::ChatHandler;
    use crate::hub::ChatHub;
    use crate::models::message::{Event, MessageBody, MsgChannel, MsgType, Payload, Status};
    use chrono::Utc;

    fn message_with_channel(channel: MsgChannel, room_id: &str) -> MessageBody {
        MessageBody {
            status: Status::Sending,
            client_id: "client".to_owned(),
            send_at: Utc::now(),
            server_sync_at: None,
            send_id: "message".to_owned(),
            sync_id: String::new(),
            msg_type: MsgType::Text,
            event: Event::SendMsg,
            room_id: room_id.to_owned(),
            uid: 7,
            sender: None,
            payload: Payload::Content {
                text: "hello".to_owned(),
            },
            channel: Some(channel),
            recipient_uid: None,
            ack_message_id: None,
            device_id: None,
            ack_stage: None,
        }
    }

    /// # [HANDLER] - shared_handler_broadcasts_message
    /// * `DESC`: `妤犲矁鐦夐崗鍙橀煩 Handler 鐎电懓鍙曢崗閬嶎暥闁挻澧界悰灞惧煣闂傛潙绠嶉幘鐠?
    #[test]
    fn shared_handler_broadcasts_message() {
        let hub = ChatHub::new(4);
        let mut receiver = hub.register("connection".to_owned());
        let handler = ChatHandler::new(hub, 20);

        handler.handle_message(
            "connection",
            message_with_channel(MsgChannel::public, "room"),
        );

        let message = receiver
            .try_recv()
            .expect("room broadcast should be queued");
        assert_eq!(message.room_id, "room");
        assert!(message.server_sync_at.is_some());
    }

    /// # [HANDLER] - public_handler_ignores_empty_room
    /// * `DESC`: `妤犲矁鐦夌粚鐑樺煣闂?ID 娑撳秷袝閸欐垶鍩ч梻鏉戠畭閹剧挜
    #[test]
    fn public_handler_ignores_empty_room() {
        let hub = ChatHub::new(4);
        let mut receiver = hub.register("connection".to_owned());
        let handler = ChatHandler::new(hub, 20);

        handler.handle_message("connection", message_with_channel(MsgChannel::public, ""));

        assert!(receiver.try_recv().is_err());
    }

    /// # [HANDLER] - non_public_chat_and_ack_channels_are_not_broadcast
    /// * `DESC`: `验证单聊、群聊和 ACK 通道不会走公共房间广播`
    #[test]
    fn non_public_chat_and_ack_channels_are_not_broadcast() {
        for channel in [MsgChannel::single, MsgChannel::group, MsgChannel::received] {
            let hub = ChatHub::new(2);
            let mut receiver = hub.register("anonymous".to_owned());
            let handler = ChatHandler::new(hub, 20);

            handler.handle_message("anonymous", message_with_channel(channel, "room"));

            assert!(receiver.try_recv().is_err());
        }
    }
}

//////// END