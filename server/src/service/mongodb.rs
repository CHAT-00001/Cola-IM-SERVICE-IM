// server/src/service/mongodb.rs -- MongoDB message, inbox and outbox repository
// 2026/10/7 21:46 Created.

////////

use mongodb::bson::{doc, from_bson, to_bson, Bson, DateTime as BsonDateTime, Document};
use mongodb::options::{IndexOptions, UpdateOptions, WriteConcern};
use mongodb::{Client, Collection, Database, IndexModel};
use tracing::error;

use crate::chat::model::{
    AckStage, ConversationKind, PendingDelivery, PersistMessage, SenderEndpoint,
};
use crate::chat::port::{ChatError, MessageStore};
use crate::models::message::MessageBody;

////////

const MAIN_MESSAGES: &str = "messages"; // 主消息
const OUTBOX_MESSAGES: &str = "outbox"; // 发件箱
const INBOX_MESSAGES: &str = "inbox"; // 收件箱

////////

/// # [MODEL] - 多文档事务策略
/// * `DESC`: `MongoDB 的多文档事务只在副本集/分片集群可用；单机 mongod 必须降级为顺序幂等写入，
///   否则服务端会返回 code 20（Transaction numbers are only allowed on a replica set member or mongos）`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionsMode {
    Auto, // 探测部署能力后决定（默认）
    On,   // 强制使用事务，配置漂移时直接暴露错误
    Off,  // 强制顺序写入
}

////////

impl TransactionsMode {
    /// # [HANDLER] - parse
    /// * `DESC`: `解析 [mongodb] transactions 配置，空值按 auto 处理`
    pub fn parse(value: &str) -> Result<Self, String> {
        let normalized = value.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "" | "auto" => Ok(Self::Auto),
            "on" | "true" | "require" => Ok(Self::On),
            "off" | "false" | "disable" => Ok(Self::Off),
            other => Err(format!(
                "mongodb.transactions 非法: {other}（可选 auto|on|off）"
            )),
        }
    }

    /// # [HANDLER] - as_str
    /// * `DESC`: `配置值名称，仅用于日志与排障`
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::On => "on",
            Self::Off => "off",
        }
    }
}

////////

/// # [HANDLER] - MongoMessageStore
/// * `DESC`: `MongoDB 消息仓储，持有主消息、发件箱和收件箱三个集合`
#[derive(Clone)]
pub struct MongoMessageStore {
    client: Client,               // MongoDB 客户端（用于开启事务会话）
    main: Collection<Document>,   // 主消息集合
    outbox: Collection<Document>, // 发送者发件箱集合
    inbox: Collection<Document>,  // 接收者收件箱集合
    transactions_enabled: bool,   // 当前部署是否支持多文档事务（standalone 为 false）
}

////////

/// # [HANDLER] - UnavailableMessageStore
/// * `DESC`: `MongoDB 不可用时的降级消息仓储，所有操作直接返回持久化错误`
#[derive(Clone)]
pub struct UnavailableMessageStore {
    reason: String, // 降级原因，写入错误信息
}

////////

/// # [HANDLER] - MessageStoreBackend
/// * `DESC`: `按 MongoDB 可用性分派的消息存储后端枚举`
#[derive(Clone)]
pub enum MessageStoreBackend {
    Available(MongoMessageStore),         // MongoDB 可用
    Unavailable(UnavailableMessageStore), // MongoDB 不可用，走降级
}

////////

#[async_trait::async_trait]
impl MessageStore for MessageStoreBackend {
    ////////

    /// # [HANDLER] - persist_message_and_mailboxes
    /// * `DESC`: `按后端状态分派消息与邮箱的持久化写入`
    async fn persist_message_and_mailboxes(
        &self,
        message: &PersistMessage,
    ) -> Result<(), ChatError> {
        match self {
            Self::Available(store) => store.persist_message_and_mailboxes(message).await,
            Self::Unavailable(store) => store.persist_message_and_mailboxes(message).await,
        }
    }

    ////////

    /// # [HANDLER] - acknowledge_device
    /// * `DESC`: `按后端状态分派设备送达/已读确认写入`
    async fn acknowledge_device(
        &self,
        message_id: &str,
        account_id: u64,
        device_id: &str,
        stage: AckStage,
    ) -> Result<(), ChatError> {
        match self {
            Self::Available(store) => {
                store
                    .acknowledge_device(message_id, account_id, device_id, stage)
                    .await
            }
            Self::Unavailable(store) => {
                store
                    .acknowledge_device(message_id, account_id, device_id, stage)
                    .await
            }
        }
    }

    ////////

    /// # [HANDLER] - pending_for_device
    /// * `DESC`: `按后端状态分派设备未确认消息查询`
    async fn pending_for_device(
        &self,
        account_id: u64,
        device_id: &str,
        limit: u32,
    ) -> Result<Vec<PendingDelivery>, ChatError> {
        match self {
            Self::Available(store) => store.pending_for_device(account_id, device_id, limit).await,
            Self::Unavailable(store) => {
                store.pending_for_device(account_id, device_id, limit).await
            }
        }
    }

    ////////

    /// # [HANDLER] - sender_endpoint
    /// * `DESC`: `按后端状态分派消息发送者账号与设备查询`
    async fn sender_endpoint(&self, message_id: &str) -> Result<SenderEndpoint, ChatError> {
        match self {
            Self::Available(store) => store.sender_endpoint(message_id).await,
            Self::Unavailable(store) => store.sender_endpoint(message_id).await,
        }
    }

    ////////

    /// # [HANDLER] - conversation_history
    /// * `DESC`: `按后端状态分派会话历史查询`
    async fn conversation_history(
        &self,
        conversation_id: &str,
        limit: u32,
        before: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<MessageBody>, ChatError> {
        match self {
            Self::Available(store) => {
                store
                    .conversation_history(conversation_id, limit, before)
                    .await
            }
            Self::Unavailable(store) => {
                store
                    .conversation_history(conversation_id, limit, before)
                    .await
            }
        }
    }
}


impl UnavailableMessageStore {
    ////////

    /// # [HANDLER] - new
    /// * `DESC`: `创建 MongoDB 不可用时的降级消息仓储`
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    ////////

    /// # [HANDLER] - error
    /// * `DESC`: `构造携带降级原因的持久化错误`
    fn error(&self) -> ChatError {
        ChatError::Persistence(format!("[🤐 MG REPO] - ❌️ MongoDB unavailable: {}", self.reason))
    }
}

#[async_trait::async_trait]
impl MessageStore for UnavailableMessageStore {
    ////////

    /// # [HANDLER] - persist_message_and_mailboxes
    /// * `DESC`: `降级状态下拒绝消息写入并记录错误日志`
    async fn persist_message_and_mailboxes(
        &self,
        _message: &PersistMessage,
    ) -> Result<(), ChatError> {
        error!(reason = %self.reason, "[🤐 MG REPO] - ❌️ 消息未持久化：MongoDB 不可用");
        Err(self.error())
    }

    ////////

    /// # [HANDLER] - acknowledge_device
    /// * `DESC`: `降级状态下拒绝设备确认写入`
    async fn acknowledge_device(
        &self,
        _message_id: &str,
        _account_id: u64,
        _device_id: &str,
        _stage: AckStage,
    ) -> Result<(), ChatError> {
        Err(self.error())
    }

    ////////

    /// # [HANDLER] - pending_for_device
    /// * `DESC`: `降级状态下拒绝未确认消息查询`
    async fn pending_for_device(
        &self,
        _account_id: u64,
        _device_id: &str,
        _limit: u32,
    ) -> Result<Vec<PendingDelivery>, ChatError> {
        Err(self.error())
    }

    ////////

    /// # [HANDLER] - sender_endpoint
    /// * `DESC`: `降级状态下拒绝发送者查询`
    async fn sender_endpoint(&self, _message_id: &str) -> Result<SenderEndpoint, ChatError> {
        Err(self.error())
    }

    ////////

    /// # [HANDLER] - conversation_history
    /// * `DESC`: `降级状态下没有可读历史，返回空列表而不是 5xx，避免客户端整块失败`
    async fn conversation_history(
        &self,
        _conversation_id: &str,
        _limit: u32,
        _before: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<MessageBody>, ChatError> {
        error!(reason = %self.reason, "[🤐 MG REPO] - ⚠️ 会话历史不可用：MongoDB 未连接");
        Ok(Vec::new())
    }
}

impl MongoMessageStore {
    ////////

    /// # [HANDLER] - from_client
    /// * `DESC`: `使用公共配置库创建的 MongoDB 客户端构建消息仓储并初始化索引；
    ///   mode 为 auto 时探测部署是否支持多文档事务，standalone 自动降级为顺序幂等写入`
    pub async fn from_client(
        client: Client,
        dbname: &str,
        mode: TransactionsMode,
    ) -> Result<Self, ChatError> {
        let transactions_enabled = match mode {
            TransactionsMode::On => true,
            TransactionsMode::Off => false,
            TransactionsMode::Auto => Self::supports_transactions(&client).await,
        };
        let database = client.database(dbname);
        let mut store = Self::from_database(client, database);
        store.transactions_enabled = transactions_enabled;
        store.ensure_indexes().await?;
        store.log_transactions_mode(mode);
        Ok(store)
    }

    ////////

    /// # [HANDLER] - supports_transactions
    /// * `DESC`: `多文档事务仅在副本集（hello 返回 setName）或分片集群（msg=isdbgrid）可用；
    ///   探测失败时保守按不支持处理，避免启动即失败`
    pub async fn supports_transactions(client: &Client) -> bool {
        match client
            .database("admin")
            .run_command(doc! { "hello": 1 })
            .await
        {
            Ok(reply) => {
                reply.get_str("setName").is_ok() || matches!(reply.get_str("msg"), Ok("isdbgrid"))
            }
            Err(error) => {
                tracing::warn!(
                    target: "server::mongo_store",
                    error = %error,
                    "[🍃 MG STORE] 无法探测 MongoDB 事务能力，按不支持处理"
                );
                false
            }
        }
    }

    ////////

    /// # [HANDLER] - log_transactions_mode
    /// * `DESC`: `打印最终生效的落库模式，便于区分事务写入与顺序降级写入`
    fn log_transactions_mode(&self, mode: TransactionsMode) {
        if self.transactions_enabled {
            tracing::info!(
                target: "server::mongo_store",
                mode = mode.as_str(),
                transactions_enabled = true,
                "[🍃 MG STORE] 消息落库使用多文档事务"
            );
        } else {
            tracing::warn!(
                target: "server::mongo_store",
                mode = mode.as_str(),
                transactions_enabled = false,
                "[🍃 MG STORE] 当前 MongoDB 不支持多文档事务（standalone），消息落库降级为顺序幂等写入"
            );
        }
    }

    ////////

    /// # [HANDLER] - from_database
    /// * `DESC`: `从已连接数据库构建消息仓储，供集成测试和应用装配使用（默认按支持事务构建）`
    pub fn from_database(client: Client, database: Database) -> Self {
        Self {
            main: database.collection(MAIN_MESSAGES),
            outbox: database.collection(OUTBOX_MESSAGES),
            inbox: database.collection(INBOX_MESSAGES),
            client,
            transactions_enabled: true,
        }
    }

    ////////

    /// # [HANDLER] - ensure_indexes
    /// * `DESC`: `为幂等消息、会话时间线和用户邮箱查询创建索引`
    pub async fn ensure_indexes(&self) -> Result<(), ChatError> {
        let unique = IndexOptions::builder().unique(true).build();
        self.main
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "message_id": 1 })
                    .options(unique.clone())
                    .build(),
            )
            .await
            .map_err(persistence_error)?;
        self.main
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "conversation_id": 1, "created_at": 1, "message_id": 1 })
                    .build(),
            )
            .await
            .map_err(persistence_error)?;
        self.outbox
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "sender_account_id": 1, "message_id": 1 })
                    .options(unique.clone())
                    .build(),
            )
            .await
            .map_err(persistence_error)?;
        self.inbox
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "recipient_account_id": 1, "message_id": 1 })
                    .options(unique)
                    .build(),
            )
            .await
            .map_err(persistence_error)?;
        self.inbox
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "recipient_account_id": 1, "created_at": 1, "message_id": 1 })
                    .build(),
            )
            .await
            .map_err(persistence_error)?;

        Ok(())
    }

    ////////

    /// # [HANDLER] - main_document
    /// * `DESC`: `把待持久化消息转换为 MongoDB 主消息文档`
    fn main_document(message: &PersistMessage) -> Result<Document, ChatError> {
        let body = to_bson(&message.body).map_err(persistence_error)?;
        Ok(doc! {
            "message_id": &message.message_id,
            "conversation_id": &message.conversation_id,
            "conversation_kind": match message.conversation_kind {
                ConversationKind::Direct => "direct",
                ConversationKind::Group => "group",
            },
            "sender_account_id": i64::try_from(message.sender_account_id)
                .map_err(|_| ChatError::InvalidMessage("sender account id exceeds BSON int64".to_owned()))?,
            "sender_device_id": &message.sender_device_id,
            "body": body,
            "recipient_account_ids": message
                .recipient_account_ids
                .iter()
                .map(|account_id| i64::try_from(*account_id)
                    .map(Bson::Int64)
                    .map_err(|_| ChatError::InvalidMessage("recipient account id exceeds BSON int64".to_owned())))
                .collect::<Result<Vec<_>, _>>()?,
            "created_at": BsonDateTime::from_millis(message.created_at.timestamp_millis()),
            "server_persisted": true,
        })
    }

    ////////

    /// # [HANDLER] - outbox_id
    /// * `DESC`: `发件箱幂等键：发送者账号 + 消息 ID`
    fn outbox_id(sender_account_id: i64, message_id: &str) -> String {
        format!("{sender_account_id}:{message_id}")
    }

    ////////

    /// # [HANDLER] - inbox_id
    /// * `DESC`: `收件箱幂等键：接收者账号 + 消息 ID`
    fn inbox_id(recipient_account_id: i64, message_id: &str) -> String {
        format!("{recipient_account_id}:{message_id}")
    }

    ////////

    /// # [HANDLER] - outbox_payload
    /// * `DESC`: `构造发件箱 $setOnInsert 文档体（事务与顺序写入共用，避免字段漂移）`
    fn outbox_payload(
        message: &PersistMessage,
        sender_account_id: i64,
        created_at: BsonDateTime,
    ) -> Document {
        doc! {
            "sender_account_id": sender_account_id,
            "message_id": &message.message_id,
            "sender_device_id": &message.sender_device_id,
            "created_at": created_at,
            "server_persisted": true,
            "device_acknowledgements": {},
        }
    }

    ////////

    /// # [HANDLER] - inbox_payload
    /// * `DESC`: `构造收件箱 $setOnInsert 文档体（事务与顺序写入共用）`
    fn inbox_payload(
        message: &PersistMessage,
        recipient_account_id: i64,
        created_at: BsonDateTime,
    ) -> Document {
        doc! {
            "recipient_account_id": recipient_account_id,
            "message_id": &message.message_id,
            "created_at": created_at,
            "server_persisted": true,
            "device_acknowledgements": {},
        }
    }

    ////////

    /// # [HANDLER] - ensure_same_message
    /// * `DESC`: `同一 message_id 重复投递必须携带相同内容，否则拒绝覆盖已落库消息`
    fn ensure_same_message(
        existing: &Document,
        message: &PersistMessage,
        main_document: &Document,
    ) -> Result<(), ChatError> {
        let requested_recipients =
            to_bson(&message.recipient_account_ids).map_err(persistence_error)?;
        let existing_body = message_body_for_retry(existing.get("body").cloned());
        let requested_body = message_body_for_retry(main_document.get("body").cloned());
        let same_message = existing.get_i64("sender_account_id").ok()
            == i64::try_from(message.sender_account_id).ok()
            && existing.get_str("sender_device_id").ok() == Some(message.sender_device_id.as_str())
            && existing.get_str("conversation_id").ok() == Some(message.conversation_id.as_str())
            && existing.get("recipient_account_ids") == Some(&requested_recipients)
            && existing_body == requested_body;
        if same_message {
            return Ok(());
        }
        Err(ChatError::InvalidMessage(
            "message id was already used for different message data".to_owned(),
        ))
    }

    ////////

    /// # [HANDLER] - sequential_write_error
    /// * `DESC`: `顺序写入失败时记录集合与消息标识，并提示可能存在部分写入`
    fn sequential_write_error(
        collection: &'static str,
        message: &PersistMessage,
        error: mongodb::error::Error,
    ) -> ChatError {
        tracing::error!(
            target: "server::mongo_store",
            message_id = %message.message_id,
            conversation_id = %message.conversation_id,
            collection,
            error = %error,
            "[🍃 MG STORE] 顺序写入失败（无事务模式）；该消息可能存在部分写入，重试会幂等补齐"
        );
        persistence_error(error)
    }

    ////////

    /// # [HANDLER] - persist_sequentially
    /// * `DESC`: `standalone 部署没有多文档事务：按主消息 -> 发件箱 -> 收件箱顺序执行幂等 upsert；
    ///   中途失败会留下部分写入，客户端重发同一 message_id 时会幂等补齐`
    async fn persist_sequentially(&self, message: &PersistMessage) -> Result<(), ChatError> {
        let main_document = Self::main_document(message)?;
        let created_at = BsonDateTime::from_millis(message.created_at.timestamp_millis());
        let sender_account_id = i64::try_from(message.sender_account_id).map_err(|_| {
            ChatError::InvalidMessage("sender account id exceeds BSON int64".to_owned())
        })?;

        // 1. 幂等校验：同一 message_id 必须携带相同内容，避免覆盖历史消息
        if let Some(existing) = self
            .main
            .find_one(doc! { "message_id": &message.message_id })
            .await
            .map_err(|error| Self::sequential_write_error("messages", message, error))?
        {
            Self::ensure_same_message(&existing, message, &main_document)?;
        }

        // 2. 主消息
        self.main
            .update_one(
                doc! { "message_id": &message.message_id },
                doc! { "$setOnInsert": main_document },
            )
            .with_options(UpdateOptions::builder().upsert(true).build())
            .await
            .map_err(|error| Self::sequential_write_error("messages", message, error))?;

        // 3. 发件箱
        let outbox_document_id = Self::outbox_id(sender_account_id, &message.message_id);
        self.outbox
            .update_one(
                doc! { "_id": &outbox_document_id },
                doc! { "$setOnInsert": Self::outbox_payload(message, sender_account_id, created_at) },
            )
            .with_options(UpdateOptions::builder().upsert(true).build())
            .await
            .map_err(|error| Self::sequential_write_error("outbox", message, error))?;

        // 4. 收件箱
        for recipient_account_id in &message.recipient_account_ids {
            let recipient = i64::try_from(*recipient_account_id).map_err(|_| {
                ChatError::InvalidMessage("recipient account id exceeds BSON int64".to_owned())
            })?;
            let inbox_document_id = Self::inbox_id(recipient, &message.message_id);
            self.inbox
                .update_one(
                    doc! { "_id": &inbox_document_id },
                    doc! { "$setOnInsert": Self::inbox_payload(message, recipient, created_at) },
                )
                .with_options(UpdateOptions::builder().upsert(true).build())
                .await
                .map_err(|error| Self::sequential_write_error("inbox", message, error))?;
        }

        tracing::info!(
            target: "server::mongo_store",
            message_id = %message.message_id,
            conversation_id = %message.conversation_id,
            conversation_kind = ?message.conversation_kind,
            sender_account_id = message.sender_account_id,
            recipient_count = message.recipient_account_ids.len(),
            collections = "messages,outbox,inbox",
            transactions_enabled = false,
            "[🍃 MG STORE] 消息已落库（无事务模式：主消息 + 发件箱 + 收件箱）"
        );
        Ok(())
    }
}

////////

#[async_trait::async_trait]
impl MessageStore for MongoMessageStore {
    ////////

    /// # [HANDLER] - persist_message_and_mailboxes
    /// * `DESC`: `幂等写入主消息、发件箱与收件人收件箱；支持事务时单事务提交，
    ///   不支持事务的 standalone 部署降级为顺序幂等写入`
    async fn persist_message_and_mailboxes(
        &self,
        message: &PersistMessage,
    ) -> Result<(), ChatError> {
        if !self.transactions_enabled {
            return self.persist_sequentially(message).await;
        }

        let main_document = Self::main_document(message)?;
        let created_at = BsonDateTime::from_millis(message.created_at.timestamp_millis());
        let sender_account_id = i64::try_from(message.sender_account_id).map_err(|_| {
            ChatError::InvalidMessage("sender account id exceeds BSON int64".to_owned())
        })?;
        let mut session = self
            .client
            .start_session()
            .await
            .map_err(persistence_error)?;
        session
            .start_transaction()
            .write_concern(WriteConcern::majority())
            .await
            .map_err(persistence_error)?;

        let transaction_result = async {
            if let Some(existing) = self
                .main
                .find_one(doc! { "message_id": &message.message_id })
                .session(&mut session)
                .await
                .map_err(persistence_error)?
            {
                Self::ensure_same_message(&existing, message, &main_document)?;
            }

            self.main
                .update_one(
                    doc! { "message_id": &message.message_id },
                    doc! { "$setOnInsert": main_document },
                )
                .with_options(UpdateOptions::builder().upsert(true).build())
                .session(&mut session)
                .await
                .map_err(persistence_error)?;

            let sender_outbox_id = Self::outbox_id(sender_account_id, &message.message_id);
            self.outbox
                .update_one(
                    doc! { "_id": &sender_outbox_id },
                    doc! { "$setOnInsert": Self::outbox_payload(message, sender_account_id, created_at) },
                )
                .with_options(UpdateOptions::builder().upsert(true).build())
                .session(&mut session)
                .await
                .map_err(persistence_error)?;

            for recipient_account_id in &message.recipient_account_ids {
                let recipient = i64::try_from(*recipient_account_id).map_err(|_| {
                    ChatError::InvalidMessage("recipient account id exceeds BSON int64".to_owned())
                })?;
                let inbox_document_id = Self::inbox_id(recipient, &message.message_id);
                self.inbox
                    .update_one(
                        doc! { "_id": &inbox_document_id },
                        doc! { "$setOnInsert": Self::inbox_payload(message, recipient, created_at) },
                    )
                    .with_options(UpdateOptions::builder().upsert(true).build())
                    .session(&mut session)
                    .await
                    .map_err(persistence_error)?;
            }

            Ok::<(), ChatError>(())
        }
        .await;

        if let Err(error) = transaction_result {
            tracing::error!(
                target: "server::mongo_store",
                message_id = %message.message_id,
                conversation_id = %message.conversation_id,
                error = %error,
                "[🍃 MG STORE] 事务写入失败，已回滚；消息未落库"
            );
            let _ = session.abort_transaction().await;
            return Err(error);
        }

        session
            .commit_transaction()
            .await
            .map_err(|error| {
                tracing::error!(
                    target: "server::mongo_store",
                    message_id = %message.message_id,
                    error = %error,
                    "[🍃 MG STORE] 事务提交失败；消息未落库"
                );
                persistence_error(error)
            })?;

        tracing::info!(
            target: "server::mongo_store",
            message_id = %message.message_id,
            conversation_id = %message.conversation_id,
            conversation_kind = ?message.conversation_kind,
            sender_account_id = message.sender_account_id,
            recipient_count = message.recipient_account_ids.len(),
            collections = "messages,outbox,inbox",
            "[🍃 MG STORE] 消息已落库（主消息 + 发件箱 + 收件箱）"
        );
        Ok(())
    }

    ////////

    /// # [HANDLER] - acknowledge_device
    /// * `DESC`: `在收件箱或发件箱上按设备记录送达/已读阶段`
    async fn acknowledge_device(
        &self,
        message_id: &str,
        account_id: u64,
        device_id: &str,
        stage: AckStage,
    ) -> Result<(), ChatError> {
        if device_id.is_empty() || device_id.len() > 128 {
            return Err(ChatError::InvalidMessage("invalid device id".to_owned()));
        }
        let account_id = i64::try_from(account_id)
            .map_err(|_| ChatError::InvalidMessage("account id exceeds BSON int64".to_owned()))?;
        let stage_value = match stage {
            AckStage::ServerPersisted => {
                return Err(ChatError::InvalidMessage(
                    "server persistence ACK cannot be supplied by a client".to_owned(),
                ));
            }
            AckStage::DeviceDelivered => 1_i32,
            AckStage::DeviceRead => 2_i32,
        };
        let device_key = hex_device_key(device_id);
        let stage_path = format!("device_acknowledgements.{device_key}.stage");
        let device_path = format!("device_acknowledgements.{device_key}.device_id");
        let timestamp_path = format!("device_acknowledgements.{device_key}.acknowledged_at");

        let update_result = self
            .inbox
            .update_one(
                doc! { "message_id": message_id, "recipient_account_id": account_id },
                doc! {
                    "$max": { (stage_path.clone()): stage_value },
                    "$set": {
                        (device_path.clone()): device_id,
                        (timestamp_path.clone()): BsonDateTime::now(),
                    },
                },
            )
            .await
            .map_err(persistence_error)?;

        if update_result.matched_count > 0 {
            return Ok(());
        }

        let outbox_update = self
            .outbox
            .update_one(
                doc! { "message_id": message_id, "sender_account_id": account_id },
                doc! {
                    "$max": { (stage_path): stage_value },
                    "$set": {
                        (device_path): device_id,
                        (timestamp_path): BsonDateTime::now(),
                    },
                },
            )
            .await
            .map_err(persistence_error)?;
        if outbox_update.matched_count == 0 {
            return Err(ChatError::MessageNotFound);
        }
        Ok(())
    }

    ////////

    /// # [HANDLER] - pending_for_device
    /// * `DESC`: `查询该设备未确认的收件箱消息，并用发件箱补足其他设备发出的消息`
    async fn pending_for_device(
        &self,
        account_id: u64,
        device_id: &str,
        limit: u32,
    ) -> Result<Vec<PendingDelivery>, ChatError> {
        let account_id = i64::try_from(account_id)
            .map_err(|_| ChatError::InvalidMessage("account id exceeds BSON int64".to_owned()))?;
        let device_key = hex_device_key(device_id);
        let stage_path = format!("device_acknowledgements.{device_key}.stage");
        let inbox_filter = doc! {
            "recipient_account_id": account_id,
            "$or": [
                { (stage_path.clone()): { "$exists": false } },
                { (stage_path.clone()): { "$lt": 1 } },
            ],
        };
        let mut cursor = self
            .inbox
            .find(inbox_filter)
            .sort(doc! { "created_at": 1, "message_id": 1 })
            .limit(i64::from(limit.clamp(1, 100)))
            .await
            .map_err(persistence_error)?;

        use futures_util::TryStreamExt;
        let inbox_documents: Vec<Document> =
            cursor.try_collect().await.map_err(persistence_error)?;
        let mut pending = Vec::with_capacity(inbox_documents.len());
        for inbox_document in &inbox_documents {
            let Some(message_id) = inbox_document.get_str("message_id").ok() else {
                continue;
            };
            let Some(main_document) = self
                .main
                .find_one(doc! { "message_id": message_id })
                .await
                .map_err(persistence_error)?
            else {
                continue;
            };
            let body = main_document.get("body").cloned().ok_or_else(|| {
                ChatError::Persistence("message body missing from main document".to_owned())
            })?;
            let message = from_bson(body).map_err(persistence_error)?;
            let recipient = inbox_document
                .get_i64("recipient_account_id")
                .map_err(persistence_error)?;
            let recipient_account_id = u64::try_from(recipient).map_err(|_| {
                ChatError::Persistence("negative recipient account id in inbox".to_owned())
            })?;
            let created_at = chrono::DateTime::<chrono::Utc>::from(
                inbox_document
                    .get_datetime("created_at")
                    .map_err(persistence_error)?
                    .to_system_time(),
            );
            pending.push(PendingDelivery {
                message_id: message_id.to_owned(),
                recipient_account_id,
                message,
                created_at,
            });
        }

        let remaining_limit = limit.saturating_sub(pending.len() as u32);
        if remaining_limit > 0 {
            let outbox_filter = doc! {
                "sender_account_id": account_id,
                "sender_device_id": { "$ne": device_id },
                "$or": [
                    { (stage_path.clone()): { "$exists": false } },
                    { (stage_path.clone()): { "$lt": 1 } },
                ],
            };
            let mut outbox_cursor = self
                .outbox
                .find(outbox_filter)
                .sort(doc! { "created_at": 1, "message_id": 1 })
                .limit(i64::from(remaining_limit))
                .await
                .map_err(persistence_error)?;
            let outbox_documents: Vec<Document> = outbox_cursor
                .try_collect()
                .await
                .map_err(persistence_error)?;
            for outbox_document in outbox_documents {
                let Some(message_id) = outbox_document.get_str("message_id").ok() else {
                    continue;
                };
                let Some(main_document) = self
                    .main
                    .find_one(doc! { "message_id": message_id })
                    .await
                    .map_err(persistence_error)?
                else {
                    continue;
                };
                let body = main_document.get("body").cloned().ok_or_else(|| {
                    ChatError::Persistence("message body missing from main document".to_owned())
                })?;
                let message = from_bson(body).map_err(persistence_error)?;
                let created_at = chrono::DateTime::<chrono::Utc>::from(
                    outbox_document
                        .get_datetime("created_at")
                        .map_err(persistence_error)?
                        .to_system_time(),
                );
                pending.push(PendingDelivery {
                    message_id: message_id.to_owned(),
                    recipient_account_id: account_id as u64,
                    message,
                    created_at,
                });
            }
        }
        Ok(pending)
    }

    ////////

    /// # [HANDLER] - sender_endpoint
    /// * `DESC`: `按消息 ID 查询原始发送者账号与设备`
    async fn sender_endpoint(&self, message_id: &str) -> Result<SenderEndpoint, ChatError> {
        let main = self
            .main
            .find_one(doc! { "message_id": message_id })
            .await
            .map_err(persistence_error)?
            .ok_or(ChatError::MessageNotFound)?;
        let account_id = main
            .get_i64("sender_account_id")
            .map_err(persistence_error)?;
        let account_id = u64::try_from(account_id)
            .map_err(|_| ChatError::Persistence("negative sender account id".to_owned()))?;
        let device_id = main
            .get_str("sender_device_id")
            .map_err(persistence_error)?
            .to_owned();
        Ok(SenderEndpoint {
            account_id,
            device_id,
        })
    }

    ////////

    /// # [HANDLER] - conversation_history
    /// * `DESC`: `按 conversation_id 从主消息集合倒序读取最近 limit 条，再反转成时间正序返回`
    /// * `NOTE`: `before 为上一页最早一条的 send_at，用于向上翻页；索引 (conversation_id, created_at, message_id) 已建`
    async fn conversation_history(
        &self,
        conversation_id: &str,
        limit: u32,
        before: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<MessageBody>, ChatError> {
        if conversation_id.trim().is_empty() {
            return Ok(Vec::new());
        }

        let mut filter = doc! { "conversation_id": conversation_id };
        if let Some(before) = before {
            filter.insert(
                "created_at",
                doc! { "$lt": BsonDateTime::from_millis(before.timestamp_millis()) },
            );
        }

        let cursor = self
            .main
            .find(filter)
            .sort(doc! { "created_at": -1, "message_id": -1 })
            .limit(i64::from(limit.clamp(1, 200)))
            .await
            .map_err(persistence_error)?;

        use futures_util::TryStreamExt;
        let documents: Vec<Document> = cursor.try_collect().await.map_err(persistence_error)?;
        let mut messages = Vec::with_capacity(documents.len());
        for document in documents {
            let Some(body) = document.get("body").cloned() else {
                continue;
            };
            let message: MessageBody = from_bson(body).map_err(persistence_error)?;
            messages.push(message);
        }

        // 数据库按时间倒序返回，聊天窗口需要旧 -> 新，因此整体反转。
        messages.reverse();
        Ok(messages)
    }
}

////////

/// # [HANDLER] - hex_device_key
/// * `DESC`: `将设备 ID 编码为 MongoDB dotted path 安全的字段键`
fn hex_device_key(device_id: &str) -> String {
    let mut key = String::with_capacity(device_id.len() * 2);
    for byte in device_id.as_bytes() {
        use std::fmt::Write;
        let _ = write!(key, "{byte:02x}");
    }
    key
}

////////

/// # [HANDLER] - persistence_error
/// * `DESC`: `将 MongoDB 与 BSON 错误转换为不泄漏凭据的仓储错误`
fn persistence_error(error: impl std::fmt::Display) -> ChatError {
    ChatError::Persistence(error.to_string())
}

////////

/// # [HANDLER] - message_body_for_retry
/// * `DESC`: `重试比对前移除服务端同步时间，避免误判消息内容不一致`
fn message_body_for_retry(body: Option<Bson>) -> Option<Bson> {
    let mut body = body?;
    let Bson::Document(document) = &mut body else {
        return Some(body);
    };
    document.remove("server_sync_at");
    Some(body)
}

////////

/// # [HANDLER] - collection_names
/// * `DESC`: `返回三类消息集合名称供 schema 测试使用`
pub fn collection_names() -> [&'static str; 3] {
    [MAIN_MESSAGES, OUTBOX_MESSAGES, INBOX_MESSAGES]
}

////////

#[cfg(test)]
mod tests {
    use super::{collection_names, hex_device_key};

    /// # [HANDLER] - collection_layout_is_explicit
    /// * `DESC`: `验证 MongoDB 主消息、发件箱和收件箱集合名称`
    #[test]
    fn collection_layout_is_explicit() {
        assert_eq!(collection_names(), ["messages", "outbox", "inbox"]);
    }

    /// # [HANDLER] - device_key_is_path_safe_and_deterministic
    /// * `DESC`: `验证设备 ID 映射键可安全用于 MongoDB dotted path`
    #[test]
    fn device_key_is_path_safe_and_deterministic() {
        assert_eq!(hex_device_key("phone.1"), "70686f6e652e31");
        assert!(!hex_device_key("phone.$1").contains('.'));
        assert!(!hex_device_key("phone.$1").contains('$'));
    }

    /// # [HANDLER] - transactions_mode_parse
    /// * `DESC`: `验证事务策略白名单解析与非法值拒绝`
    #[test]
    fn transactions_mode_parse() {
        use super::TransactionsMode;

        assert_eq!(TransactionsMode::parse("auto"), Ok(TransactionsMode::Auto));
        assert_eq!(TransactionsMode::parse(" ON "), Ok(TransactionsMode::On));
        assert_eq!(TransactionsMode::parse("Off"), Ok(TransactionsMode::Off));
        assert_eq!(TransactionsMode::parse(""), Ok(TransactionsMode::Auto));
        assert!(TransactionsMode::parse("sometimes").is_err());
        assert_eq!(TransactionsMode::Auto.as_str(), "auto");
        assert_eq!(TransactionsMode::On.as_str(), "on");
        assert_eq!(TransactionsMode::Off.as_str(), "off");
    }

    /// # [HANDLER] - mailbox_idempotency_keys
    /// * `DESC`: `验证发件箱/收件箱幂等键为"账号:消息ID"，重试时不会产生重复文档`
    #[test]
    fn mailbox_idempotency_keys() {
        use super::MongoMessageStore;

        assert_eq!(
            MongoMessageStore::outbox_id(1_000_020_104, "m-1"),
            "1000020104:m-1"
        );
        assert_eq!(MongoMessageStore::inbox_id(42, "m-1"), "42:m-1");
    }

    /// # [HANDLER] - live_standalone_persist_roundtrip
    /// * `DESC`: `真实 MongoDB 自检：探测事务能力 → 无事务顺序幂等落库 → 重复投递不产生重复文档 → 清理探测数据`
    /// * `NOTE`: `读取 application.toml [mongodb] 配置真实地址：
    ///   cargo test -p cola-im-chat-server --bins live_standalone -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "requires the live MongoDB instance configured in application.toml [mongodb]"]
    async fn live_standalone_persist_roundtrip() {
        use super::{MongoMessageStore, TransactionsMode};
        use crate::chat::model::{ConversationKind, PersistMessage};
        use crate::chat::port::MessageStore;
        use crate::models::message::{Event, MessageBody, MsgType, Payload, Status};
        use mongodb::bson::doc;

        let settings: crate::config::Settings =
            toml::from_str(include_str!("../../../application.toml"))
                .expect("application.toml 应可解析");
        let mongo = settings.mongodb;
        let uri = format!(
            "mongodb://{}:{}@{}:{}/?authSource={}",
            mongo.user,
            mongo.password,
            mongo.host,
            mongo.port,
            mongo
                .auth_source
                .as_deref()
                .unwrap_or(mongo.dbname.as_str())
        );
        let client = mongodb::Client::with_uri_str(&uri)
            .await
            .expect("MongoDB 应可连接");

        // 1. 单机 mongod 不具备多文档事务能力，auto 模式必须降级
        assert!(
            !MongoMessageStore::supports_transactions(&client).await,
            "standalone mongod 不应被判定为支持多文档事务"
        );
        let store =
            MongoMessageStore::from_client(client.clone(), &mongo.dbname, TransactionsMode::Auto)
                .await
                .expect("消息仓储应可构建");

        // 2. 合成探测消息，重复投递两次（模拟客户端重发）
        let message_id = format!("live-standalone-{}", chrono::Utc::now().timestamp_millis());
        let sender: u64 = 9_990_000_001;
        let recipient: u64 = 9_990_000_002;
        let message = PersistMessage {
            message_id: message_id.clone(),
            conversation_id: format!("direct:{sender}:{recipient}"),
            conversation_kind: ConversationKind::Direct,
            sender_account_id: sender,
            sender_device_id: "live-device".to_owned(),
            body: MessageBody {
                status: Status::Sending,
                client_id: "live-client".to_owned(),
                send_at: chrono::Utc::now(),
                server_sync_at: None,
                send_id: message_id.clone(),
                sync_id: message_id.clone(),
                msg_type: MsgType::Text,
                event: Event::SendMsg,
                room_id: String::new(),
                uid: sender,
                sender: None,
                payload: Payload::Content {
                    text: "live standalone persist probe".to_owned(),
                },
                channel: None,
                recipient_uid: Some(recipient),
                ack_message_id: None,
                device_id: None,
                ack_stage: None,
            },
            recipient_account_ids: vec![recipient],
            created_at: chrono::Utc::now(),
        };
        store
            .persist_message_and_mailboxes(&message)
            .await
            .expect("首次落库应成功");
        store
            .persist_message_and_mailboxes(&message)
            .await
            .expect("重复落库应幂等成功");

        // 3. 三类集合各只有一条文档（顺序写入 + 幂等键生效）
        for (name, collection) in [
            ("messages", &store.main),
            ("outbox", &store.outbox),
            ("inbox", &store.inbox),
        ] {
            let count = collection
                .count_documents(doc! { "message_id": &message_id })
                .await
                .expect("计数应成功");
            assert_eq!(count, 1, "{name} 集合应幂等只保留一条文档");
        }

        // 4. 同一 message_id 携带不同内容必须被拒绝
        let mut conflicting = message.clone();
        conflicting.recipient_account_ids = vec![9_990_000_003];
        assert!(
            store
                .persist_message_and_mailboxes(&conflicting)
                .await
                .is_err(),
            "同一 message_id 携带不同内容应被拒绝"
        );

        // 5. 清理探测数据，不污染真实消息
        let filter = doc! { "message_id": &message_id };
        for collection in [&store.main, &store.outbox, &store.inbox] {
            collection
                .delete_many(filter.clone())
                .await
                .expect("清理探测数据应成功");
        }
    }
}

//////// END
