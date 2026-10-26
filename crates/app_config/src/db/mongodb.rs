// crates/app/src/db/mongodb.rs -- 板条箱 - 应用配置 - db - mongodb连接
// 2025-12-10 10:41

////////

use crate::config::Mongodb;
pub use mongodb::{options::ClientOptions, Client};
use tracing::info;

////////

/// # [CONNECT] - MONGODB
/// * `desc`: `初始化 MongoDB 连接`
pub async fn mongodb_init(config: &Mongodb) -> Result<Client, mongodb::error::Error> {
    ////////

    // 💡 - 接入点
    let endpoint = if config.port != 0 {
        format!("{}:{}", config.host, config.port)
    } else {
        config.host.clone()
    };
    let mut uri = format!("mongodb://{endpoint}");
    if let (Some(user), Some(password)) = (&config.user, &config.password) {
        if !user.is_empty() && !password.is_empty() {
            uri = format!("mongodb://{user}:{password}@{endpoint}");
        }
    }
    if let Some(source) = config
        .auth_source
        .as_deref()
        .filter(|source| !source.is_empty())
    {
        uri.push_str(&format!("/?authSource={source}"));
    } else {
        uri.push('/');
    }

    ////////

    // 💡 - 解析配置
    let mut client_options = ClientOptions::parse(&uri).await?;
    client_options.app_name = Some("LiveChatServer".to_string());
    client_options.max_pool_size = config.max_connections;
    client_options.repl_set_name = config.replica_set.clone();

    ////////

    // 💡 - 创建 MongoDB 客户端
    let client = Client::with_options(client_options)?;

    ////////

    // 💡 - LOGGER - 日志
    info!(
        "[✅️ APP CONFIG] - MongoDB connected to {}:{}",
        config.host, config.port
    );

    Ok(client)
}

//////// END
