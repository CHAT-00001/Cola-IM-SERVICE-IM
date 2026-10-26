// app/src/db.rs -- 应用配置 - 数据库连接池
// 2025-12-16 10:50

////////

pub use self::mongodb::{mongodb_init, Client as MongoClient};
pub use self::pg::pg_init;
pub use self::redis::{redis_init, Client as RedisClient};
use crate::config::AppConfig;
use sqlx::PgPool;
use tracing::error;

////////
pub mod mongodb;
pub mod pg;
pub mod redis;

////////

/// # [CONN] - 封装后的数据库连接结构体
#[derive(Clone)]
pub struct DbConnection {
    pub pg_pool: PgPool,                   // PG
    pub redis_client: RedisClient,         // REDIS
    pub mongo_client: Option<MongoClient>, // MONGODB（可选，不阻断服务启动）
}

/// # [INIT] - 初始化所有数据库连接
/// * 返回 Result<DbConnection, ()>，初始化失败打印错误并返回 Err
pub async fn db_connection(config: &AppConfig) -> Result<DbConnection, ()> {
    // --- PostgreSQL ---
    let pg_pool = match pg_init(&config.pg).await {
        Ok(pool) => pool,
        Err(e) => {
            error!("PostgreSQL initialization failed: {:?}", e);
            return Err(());
        }
    };

    // --- Redis ---
    let redis_client = match redis_init(&config.redis) {
        Ok(client) => client,
        Err(e) => {
            error!("Redis initialization failed: {:?}", e);
            return Err(());
        }
    };

    // --- MongoDB ---
    let mongo_client = match mongodb_init(&config.mongodb).await {
        Ok(client) => Some(client),
        Err(e) => {
            error!("MongoDB initialization failed: {:?}", e);
            None
        }
    };

    Ok(DbConnection {
        pg_pool,
        redis_client,
        mongo_client,
    })
}

//////// END
