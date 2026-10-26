// crates/app_config/src/db/pg.rs -- 板条箱 - 应用配置 - db - pg数据库连接
// 2026-01-24 10:30

////////

use crate::config::Pg;
use sqlx::{postgres::PgPoolOptions, PgPool};
use tracing::{error, info};

////////

/// # [CONNECT] - PG
/// * `desc`: `初始化 PostgresSQL 连接池`
pub async fn pg_init(config: &Pg) -> Result<PgPool, sqlx::Error> {
    ////////

    // 💡 - 构建 PostgresSQL 连接字符串
    let uri = format!(
        "postgres://{}:{}@{}:{}/{}",
        config.username, config.password, config.host, config.port, config.database
    );

    ////////

    // 💡 - 设置最大连接数，如果未设置，则默认 5
    let max_connections = config.max_connections.unwrap_or(5);

    match PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(&uri)
        .await
    {
        Ok(pool) => {
            info!(
                "🛢️ [PG] connected to {}:{} (database: {})",
                config.host, config.port, config.database
            );
            println!("[✅️ PG] - 数据库连接成功了");
            Ok(pool)
        }
        Err(e) => {
            error!(
                "🛢️ [PG] Failed to connect PostgresSQL {}:{} (database: {}): {:?}",
                config.host, config.port, config.database, e
            );
            Err(e)
        }
    }
}

//////// END
