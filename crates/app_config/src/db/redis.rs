// crates/app_config/src/db/redis.rs -- 板条箱 - 应用配置 - db - redis 初始化连接
// 2026-01-01 08:45:56

////////

use crate::config::Redis;
pub use redis::{Client, RedisError};
use tracing::{error, info};

////////

/// # [CONNECT] - REDIS
/// * `desc`: `初始化 Redis 客户端`
pub fn redis_init(config: &Redis) -> Result<Client, RedisError> {
    ////////

    // 💡 - 构建 Redis 连接字符串
    let uri = if let Some(user) = &config.user {
        if let Some(password) = &config.password {
            format!(
                "redis://{}:{}@{}:{}/{}",
                user,
                password,
                config.host,
                config.port,
                config.db.unwrap_or(0)
            )
        } else {
            format!(
                "redis://{}@{}:{}/{}",
                user,
                config.host,
                config.port,
                config.db.unwrap_or(0)
            )
        }
    } else if let Some(password) = &config.password {
        format!(
            "redis://:{}@{}:{}/{}",
            password,
            config.host,
            config.port,
            config.db.unwrap_or(0)
        )
    } else {
        format!(
            "redis://{}:{}/{}",
            config.host,
            config.port,
            config.db.unwrap_or(0)
        )
    };

    match Client::open(uri.as_str()) {
        Ok(client) => {
            info!("Redis connected to {}:{}", config.host, config.port);
            Ok(client)
        }
        Err(e) => {
            error!("Failed to connect Redis: {:?}", e);
            Err(e)
        }
    }
}

//////// END
