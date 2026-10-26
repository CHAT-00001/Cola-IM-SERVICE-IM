// server/src/app.rs -- SERVER: WebSocket 服务启动器
// 2026-01-04 19:14:30

////////

use actix_web::{web, App, HttpServer};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

////////

use crate::chat::service::{AcknowledgementService, DirectMessageService, GroupMessageService};
use crate::chat::session::DirectChatSessionService;
use crate::chat::ticket::TicketStore;
use crate::config::Settings;
use crate::handlers::ws_index;
use crate::hub::ChatHub;
use crate::service::blocklist::AllowAllBlocklist;
use crate::service::contact_sync::{DirectMessagePolicy, SyncingContactGateway};
use crate::service::identity::IdentitySource;
use crate::service::mongodb::{
    MessageStoreBackend, MongoMessageStore, TransactionsMode, UnavailableMessageStore,
};
use crate::service::relationships::{PgGroupGateway, PgRelationshipGateway};
use crate::shared_handler::ChatHandler;
use
app_config::app_state::AppState as ConfigAppState;
use app_config::config::{Mongodb, Pg, Redis};
use app_config::db::{mongodb_init, pg_init, redis_init, DbConnection};
use storage_pg::repository::chat_session::PgDirectChatSessionRepository;

////////

/// # [HANDLER] - run
/// * `DESC`: `加载配置和依赖并启动 WebSocket 接入服务`
pub async fn run() -> std::io::Result<()> {
    /////////

    // 💡 - START SERVER
    info!("🟢 - 正在启动 COLA IM Server ..");

    // 1. 加载配置
    info!("正在加载应用配置");
    let settings = Settings::new().expect("无法加载配置文件 application.toml");
    info!("应用配置加载完成");

    // 1.1 校验 IM 单聊准入与资料服务配置，非法配置直接启动失败而不是静默降级
    if let Err(error) = settings.im.validate() {
        error!("IM 单聊准入/资料服务配置非法: {error}");
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, error));
    }
    let direct_policy = match DirectMessagePolicy::parse(&settings.im.direct.policy) {
        Ok(policy) => policy,
        Err(error) => {
            error!("IM 单聊准入策略非法: {error}");
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, error));
        }
    };
    let identity_source = match IdentitySource::from_settings(&settings.im.identity) {
        Ok(source) => source,
        Err(error) => {
            error!("IM 资料服务传输配置非法: {error}");
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, error));
        }
    };
    info!(
        policy = direct_policy.as_str(),
        transport = %settings.im.identity.transport,
        base_url = %settings.im.identity.base_url,
        "IM 单聊准入与资料服务配置加载完成"
    );

    // 1.2 校验 MongoDB 落库模式（auto=探测副本集能力，单机自动降级为顺序幂等写入）
    let transactions_mode = match TransactionsMode::parse(&settings.mongodb.transactions) {
        Ok(mode) => mode,
        Err(error) => {
            error!("MongoDB 事务策略非法: {error}");
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, error));
        }
    };
    info!(
        mode = transactions_mode.as_str(),
        replica_set = settings.mongodb.replica_set.as_deref().unwrap_or("-"),
        "MongoDB 落库模式配置加载完成"
    );

    // 2. 通过 app_config 初始化 PostgreSQL 连接池
    info!(
        host = %settings.pg.host,
        port = settings.pg.port,
        database = %settings.pg.dbname,
        "正在连接 PostgreSQL"
    );
    let pg_pool = pg_init(&Pg {
        host: settings.pg.host.clone(),
        port: settings.pg.port as u16,
        username: settings.pg.user.clone(),
        password: settings.pg.password.clone(),
        database: settings.pg.dbname.clone(),
        max_connections: Some(settings.pg.max_connections),
    })
    .await
    .expect("无法连接到 PostgreSQL");
    info!("[🗣️ APP BOOT] - ✅️ - PostgreSQL 连接池初始化完成");

    // 3. 通过 app_config 初始化 Redis 客户端
    let redis_client = redis_init(&Redis {
        host: settings.redis.host.clone(),
        port: settings.redis.port,
        user: None,
        password: settings.redis.password.clone(),
        db: settings.redis.db.parse().ok(),
    })
    .expect("无法创建 Redis 客户端");
    info!(
        host = %settings.redis.host,
        port = settings.redis.port,
        database = settings.redis.db,
        "Redis 客户端创建完成"
    );

    info!(
        host = %settings.mongodb.host,
        port = settings.mongodb.port,
        database = %settings.mongodb.dbname,
        "正在连接 MongoDB 消息存储"
    );
    // 4. 通过 app_config 初始化 MongoDB 客户端，业务仓储只负责集合和索引
    let mongo_client = match mongodb_init(&Mongodb {
        host: settings.mongodb.host.clone(),
        port: settings.mongodb.port,
        user: Some(settings.mongodb.user.clone()),
        password: Some(settings.mongodb.password.clone()),
        database: settings.mongodb.dbname.clone(),
        auth_source: settings.mongodb.auth_source.clone(),
        max_connections: Some(settings.mongodb.max_connections),
        replica_set: settings.mongodb.replica_set.clone(),
    })
    .await
    {
        Ok(client) => Some(client),
        Err(error) => {
            error!(error = ?error, "[🤐 APP BOOT] - ❌️ - MongoDB 客户端初始化失败，继续启动服务");
            None
        }
    };
    let (mongo_client_for_state, message_store) = match mongo_client {
        Some(mongo_client) => {
            let state_client = mongo_client.clone();
            match MongoMessageStore::from_client(
                mongo_client,
                &settings.mongodb.dbname,
                transactions_mode,
            )
            .await
            {
                Ok(store) => (Some(state_client), MessageStoreBackend::Available(store)),
                Err(error) => {
                    error!(error = %error, "[🤐 APP BOOT] - ❗️ - MongoDB 索引初始化失败，继续启动服务");
                    (
                        Some(state_client),
                        MessageStoreBackend::Unavailable(UnavailableMessageStore::new(
                            error.to_string(),
                        )),
                    )
                }
            }
        }
        None => (
            None,
            MessageStoreBackend::Unavailable(UnavailableMessageStore::new(
                "client initialization failed",
            )),
        ),
    };

    // 5. 初始化应用状态
    let db = DbConnection {
        pg_pool,
        redis_client,
        mongo_client: mongo_client_for_state,
    };
    let state = Arc::new(ConfigAppState::new(
        db,
        message_store.clone(),
        settings.clone(),
        reqwest::Client::new(),
    ));

    let hub = ChatHub::new(settings.server.ws.outbound_queue_capacity);
    let chat_session_repository = PgDirectChatSessionRepository::new(state.db.clone());
    let direct_service = DirectMessageService::new(
        message_store.clone(),
        SyncingContactGateway::new(
            PgRelationshipGateway::new(state.db.clone()),
            identity_source,
            direct_policy,
        ),
        hub.clone(),
        chat_session_repository.clone(),
        AllowAllBlocklist,
    );
    let chat_session_service = Arc::new(DirectChatSessionService::new(
        chat_session_repository,
        AllowAllBlocklist,
    ));
    let group_service = GroupMessageService::new(
        message_store.clone(),
        PgGroupGateway::new(state.db.clone()),
        hub.clone(),
    );
    let acknowledgement_service = AcknowledgementService::new(message_store);
    let handler = Arc::new(
        ChatHandler::new(hub.clone(), settings.server.ws.max_rooms_per_connection)
            .with_chat_services(direct_service, group_service, acknowledgement_service),
    );

    ////////

    info!("💡💡 - 应用状态和聊天服务初始化完成");

    let state_data = web::Data::new(state);
    let hub_data = web::Data::new(hub);
    let handler_data = web::Data::new(handler);
    // 票据接口与 WS 同在主服务端口，因此消息大小上限统一取 chat_gateway 配置。
    let mut ws_settings = settings.server.ws.clone();
    ws_settings.max_message_bytes = settings.chat_gateway.max_message_bytes;
    let ws_settings_data = web::Data::new(ws_settings);
    let chat_session_service_data = web::Data::new(chat_session_service);

    // 聊天长连接服务配置非法时直接启动失败，不静默降级到匿名连接。
    if let Err(error) = settings.chat_gateway.validate() {
        error!("聊天长连接配置非法: {error}");
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, error));
    }
    let tickets = Arc::new(TicketStore::new(Duration::from_secs(
        settings.chat_gateway.ticket_ttl_seconds,
    )));
    let tickets_data = web::Data::new(tickets);

    // 6. 启动服务器
    let addr = format!(
        "{}:{}",
        settings.server.ws_host(),
        settings.server.ws_port()
    );

    info!("✅️✅️ - WS listener configured at {}", addr);
    info!("正在绑定 WebSocket 监听器");

    ////////

    info!("😊 😊 😊 😊 ✅️ ✅️ ✅️ ✅️ All services started, waiting for them to run...");
    info!("████████████░░░░░░░░  60% 🟡");
    info!("████████████████████  100% 🟡");

    ////////

    let main_server = HttpServer::new(move || {
        App::new()
            .wrap(crate::middleware::RequestLogger)
            .app_data(state_data.clone())
            .app_data(hub_data.clone())
            .app_data(handler_data.clone())
            .app_data(ws_settings_data.clone())
            .app_data(chat_session_service_data.clone())
            .app_data(tickets_data.clone())
            .route("/ws", web::get().to(ws_index))
            .route(
                "/ws/ticket",
                web::post().to(crate::ticket_handlers::issue_ws_ticket),
            )
            .route(
                "/chat/session/add",
                web::post().to(crate::session_handlers::add_direct_chat_session),
            )
            .route(
                "/chat/session/list",
                web::get().to(crate::session_handlers::list_direct_chat_sessions),
            )
            .route(
                "/message/sync",
                web::get().to(crate::message_handlers::sync_offline_messages),
            )
            .route(
                "/message/send",
                web::post().to(crate::message_handlers::send_message),
            )
    })
    .bind(&addr)?
    .run();

    main_server.await?;
    Ok(())
}

//////// END
