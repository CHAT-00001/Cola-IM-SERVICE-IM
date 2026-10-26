// app/src/db_service.rs  -- 应用配置 - 构建所有数据库连接服务
// 2025-12-20 14:10

////////

use crate::config::{AppConfig, Mongodb, Pg, Redis};
use mongodb::Client as MongoClient;
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, Client as RedisClient, RedisError};
use serde::{Serialize, de::DeserializeOwned};
use sqlx::PgPool;
use std::io;
use tracing::{error, info};

////////

/// # [SERVICE] - 数据库服务
#[derive(Clone)]
pub struct DbService {
    pub pg_pool: PgPool,                   // PostgreSQL 16+
    pub mongo_client: MongoClient,         // MongoDB 14+
    pub redis_conn: MultiplexedConnection, // Redis 6+
}

// 构造实现
impl DbService {
    ////////

    /// [CASE] - NEW
    pub async fn new(config: &AppConfig) -> Option<Self> {
        // PostgreSQL
        let pg_pool = match crate::db::pg::pg_init(&config.pg).await {
            Ok(pool) => pool,
            Err(e) => {
                error!("❌️ Failed to connect PostgreSQL: {:?}", e);
                return None;
            }
        };

        // MongoDB
        let mongo_client = match crate::db::mongodb::mongodb_init(&config.mongodb).await {
            Ok(client) => client,
            Err(e) => {
                error!("❌️ Failed to connect MongoDB: {:?}", e);
                return None;
            }
        };

        let redis_client = crate::db::redis::redis_init(&config.redis).ok()?;
        let redis_conn = redis_client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| {
                error!("❌️ Redis Connection Error: {}", e);
                e
            })
            .ok()?;

        info!("🚀 All database services connected successfully");

        Some(Self {
            pg_pool,
            // redis_client,
            mongo_client,
            redis_conn,
        })
    }

    ////////

    /// # [REDIS] - 获取缓存
    /// * `desc`: `从 Redis 获取 JSON 并反序列化`
    pub async fn get_redis_json<T: DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<T>, RedisError> {
        let mut conn = self.redis_conn.clone();
        let json_str: Option<String> = conn.get(key).await?;

        match json_str {
            Some(s) => {
                serde_json::from_str(&s)
                    .map_err(|e| {
                        // 绕过具体枚举名，直接通过 io::Error 中转
                        // 这是 Rust 处理不同库之间错误转换的“万能钥匙”
                        RedisError::from(std::io::Error::new(std::io::ErrorKind::Other, e))
                    })
                    .map(Some)
            }
            None => Ok(None),
        }
    }

    ////////

    /// # [REDIS] - 写入缓存
    /// * `desc`: `写 JSON 到 Redis 并设置 TTL`
    pub async fn set_redis_json<T: Serialize>(
        &self,
        key: &str,
        value: &T,
        ttl_secs: u64,
    ) -> Result<(), RedisError> {
        let mut conn = self.redis_conn.clone();

        let json = serde_json::to_string(value)
            .map_err(|e| RedisError::from(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        // ✅ 修正：只需要提供一个泛型参数 ()
        redis::cmd("SETEX")
            .arg(key)
            .arg(ttl_secs)
            .arg(json)
            .query_async::<()>(&mut conn)
            .await?;

        Ok(())
    }
}

//////// END
