// crates/app_config/src/app_state.rs -- 通用应用状态初始化
// 2026/10/8 01:28 Created.

////////

use crate::db::{DbConnection, MongoClient, RedisClient};
use sqlx::PgPool;

////////

/// # [HANDLER] - AppState
/// * `DESC`: `统一持有数据库连接、消息仓储、应用配置和 HTTP 客户端`
#[derive(Clone)]
pub struct AppState<C, M> {
    pub db: PgPool,
    pub redis_client: RedisClient,
    pub mongo_client: Option<MongoClient>,
    pub messages: M,
    pub config: C,
    pub http_client: reqwest::Client,
}

impl<C, M> AppState<C, M> {
    /// # [HANDLER] - new
    /// * `DESC`: `组装应用运行所需的共享状态`
    pub fn new(db: DbConnection, messages: M, config: C, http_client: reqwest::Client) -> Self {
        Self {
            db: db.pg_pool,
            redis_client: db.redis_client,
            mongo_client: db.mongo_client,
            messages,
            config,
            http_client,
        }
    }
}

//////// END
