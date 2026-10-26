// crates/app_config/src/config_loader.rs -- 板条箱 - 应用配置 - 全局配置文件加载器
// 2026-01-02 11:20

////////

use crate::config::AppConfig;
use config::Config;
use tracing::error;

////////

/// 从 application.toml 加载配置
pub fn load_config() -> Result<AppConfig, config::ConfigError> {
    let builder = Config::builder()
        .add_source(config::File::with_name("application.toml"))
        .add_source(config::Environment::with_prefix("APP")); // 支持环境变量覆盖

    let cfg = builder.build()?;

    // 反序列化到 AppConfig
    match cfg.try_deserialize::<AppConfig>() {
        Ok(app_config) => Ok(app_config),
        Err(e) => {
            error!("[❌️ LOADER] - Failed to deserialize AppConfig: {:?}", e);
            Err(e)
        }
    }
}

//////// END
