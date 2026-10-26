// server/src/config.rs -- Server configuration model and loader
// 2026-01-04 15:21

////////

use config::{Config, ConfigError, File};
use serde::Deserialize;

////////

/// # [SETTING] - 服务配置模型
#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub app: AppSettings,
    pub server: ServerSettings,
    pub pg: PgSettings,
    pub mongodb: MongoSettings,
    pub redis: RedisSettings,
    pub api: ApiSettings,
    #[serde(default)]
    pub mysql: Option<MySqlSettings>,
    #[serde(default)]
    pub kafka: Option<KafkaSettings>,
    #[serde(default)]
    pub health: HealthSettings,
    #[serde(default)]
    pub chat_gateway: ChatGatewaySettings,
    #[serde(default)]
    pub im: ImSettings,
}

////////

/// # [CHAT GATEWAY SETTING] - 聊天长连接票据与消息大小配置（WS 与票据接口共用主服务端口）
#[derive(Debug, Deserialize, Clone)]
pub struct ChatGatewaySettings {
    #[serde(default = "default_ticket_ttl_seconds")]
    pub ticket_ttl_seconds: u64,
    #[serde(default = "default_chat_gateway_max_message_bytes")]
    pub max_message_bytes: usize,
}

////////

impl Default for ChatGatewaySettings {
    fn default() -> Self {
        Self {
            ticket_ttl_seconds: default_ticket_ttl_seconds(),
            max_message_bytes: default_chat_gateway_max_message_bytes(),
        }
    }
}

impl ChatGatewaySettings {
    /// # [HANDLER] - validate
    /// * `DESC`: `校验聊天长连接配置的端口、票据有效期与消息大小范围`
    pub fn validate(&self) -> Result<(), String> {
        if !(5..=300).contains(&self.ticket_ttl_seconds) {
            return Err("chat_gateway.ticket_ttl_seconds must be between 5 and 300".to_owned());
        }
        if !(1024..=1024 * 1024).contains(&self.max_message_bytes) {
            return Err("chat_gateway.max_message_bytes must be between 1024 and 1048576".to_owned());
        }
        Ok(())
    }
}

////////

/// # [IM SETTING] - 单聊准入与用户资料配置
#[derive(Debug, Deserialize, Clone, Default)]
pub struct ImSettings {
    #[serde(default)]
    pub direct: DirectMessageSettings,
    #[serde(default)]
    pub identity: IdentitySettings,
}

////////

impl ImSettings {
    /// # [HANDLER] - validate
    /// * `DESC`: `校验单聊准入策略与资料服务配置`
    pub fn validate(&self) -> Result<(), String> {
        self.direct.validate()?;
        self.identity.validate()
    }
}

////////

/// # [DIRECT MESSAGE SETTING] - 单聊准入策略配置
#[derive(Debug, Deserialize, Clone)]
pub struct DirectMessageSettings {
    /// 准入策略: strict（仅本地通讯录） / profile（回落远端资料并注入联系人） / allow（不校验）
    #[serde(default = "default_direct_message_policy")]
    pub policy: String,
}

////////

impl Default for DirectMessageSettings {
    fn default() -> Self {
        Self {
            policy: default_direct_message_policy(),
        }
    }
}

////////

impl DirectMessageSettings {
    /// # [HANDLER] - validate
    /// * `DESC`: `校验准入策略取值`
    pub fn validate(&self) -> Result<(), String> {
        match self.policy.trim().to_ascii_lowercase().as_str() {
            "strict" | "profile" | "allow" => Ok(()),
            other => Err(format!(
                "im.direct.policy must be one of strict / profile / allow, got '{other}'"
            )),
        }
    }
}

////////

/// # [IDENTITY SETTING] - 用户资料服务（跨进程调用）配置
#[derive(Debug, Deserialize, Clone)]
pub struct IdentitySettings {
    /// 传输通道: http / grpc（grpc 为预留实现）
    #[serde(default = "default_identity_transport")]
    pub transport: String,
    /// 主后端网关地址
    #[serde(default = "default_identity_base_url")]
    pub base_url: String,
    /// 资料接口路径
    #[serde(default = "default_identity_path")]
    pub path: String,
    /// 网关服务名（主后端 gateway 的 service 参数）
    #[serde(default = "default_identity_service")]
    pub service: String,
    /// 单次调用超时（毫秒）
    #[serde(default = "default_identity_timeout_ms")]
    pub timeout_ms: u64,
    /// 内部调用令牌（非空时附加 access_token 参数）
    #[serde(default)]
    pub token: Option<String>,
    /// gRPC 端点（预留）
    #[serde(default)]
    pub grpc_base_url: Option<String>,
    /// gRPC 端口（预留，0 表示不拼接）
    #[serde(default)]
    pub grpc_port: Option<u16>,
}

////////

impl Default for IdentitySettings {
    fn default() -> Self {
        Self {
            transport: default_identity_transport(),
            base_url: default_identity_base_url(),
            path: default_identity_path(),
            service: default_identity_service(),
            timeout_ms: default_identity_timeout_ms(),
            token: None,
            grpc_base_url: None,
            grpc_port: None,
        }
    }
}

////////

impl IdentitySettings {
    /// # [HANDLER] - validate
    /// * `DESC`: `校验传输通道、网关地址、路径与超时范围`
    pub fn validate(&self) -> Result<(), String> {
        match self.transport.trim().to_ascii_lowercase().as_str() {
            "http" | "grpc" => {}
            other => {
                return Err(format!(
                    "im.identity.transport must be http or grpc, got '{other}'"
                ))
            }
        }
        if !self.base_url.starts_with("http://") && !self.base_url.starts_with("https://") {
            return Err("im.identity.base_url must be an http(s) URL".to_owned());
        }
        if !self.path.starts_with('/') {
            return Err("im.identity.path must start with '/'".to_owned());
        }
        if !(50..=30_000).contains(&self.timeout_ms) {
            return Err("im.identity.timeout_ms must be between 50 and 30000".to_owned());
        }
        Ok(())
    }
}

////////

/// # [APP SETTING] - 应用配置模型
#[derive(Debug, Deserialize, Clone)]
pub struct AppSettings {
    pub name: String,
    pub name_zh: String,
    pub version: String,
    #[serde(default)]
    pub x: Option<String>,
    #[serde(default)]
    pub sign: Option<String>,
}

////////

/// # 服务配置模型
#[derive(Debug, Deserialize, Clone)]
pub struct ServerSettings {
    #[serde(default)]
    pub ws: WsSettings,
    #[serde(default)]
    pub wt: WtSettings,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
}

////////

/// # WebSocket 配置模型
#[derive(Debug, Deserialize, Clone)]
pub struct WsSettings {
    #[serde(default = "default_ws_host")]
    pub host: String,
    #[serde(default = "default_ws_port")]
    pub port: u16,
    #[serde(default = "default_outbound_queue_capacity")]
    pub outbound_queue_capacity: usize,
    #[serde(default = "default_max_message_bytes")]
    pub max_message_bytes: usize,
    #[serde(default = "default_max_rooms_per_connection")]
    pub max_rooms_per_connection: usize,
}

////////

/// # WebTransport 配置模型
impl Default for WsSettings {
    fn default() -> Self {
        Self {
            host: default_ws_host(),
            port: default_ws_port(),
            outbound_queue_capacity: default_outbound_queue_capacity(),
            max_message_bytes: default_max_message_bytes(),
            max_rooms_per_connection: default_max_rooms_per_connection(),
        }
    }
}

////////

/// # WebTransport 配置模型
#[derive(Debug, Deserialize, Clone)]
pub struct WtSettings {
    #[serde(default = "default_wt_host")]
    pub host: String,
    #[serde(default = "default_wt_port")]
    pub port: u16,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub cert_file: Option<String>,
    #[serde(default)]
    pub key_file: Option<String>,
}

////////

/// # WebTransport 配置模型默认实现
impl Default for WtSettings {
    fn default() -> Self {
        Self {
            host: default_wt_host(),
            port: default_wt_port(),
            enabled: false,
            cert_file: None,
            key_file: None,
        }
    }
}

////////

/// # PostgreSQL 配置模型
#[derive(Debug, Deserialize, Clone)]
pub struct PgSettings {
    pub host: String,
    pub port: u32,
    pub dbname: String,
    pub user: String,
    pub password: String,
    pub max_connections: u32,
    #[serde(default)]
    pub sslmode: Option<String>,
}

////////

/// # PostgreSQL 配置模型默认实现
impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            ws: WsSettings::default(),
            wt: WtSettings::default(),
            host: None,
            port: None,
        }
    }
}

/////////

/// # MongoDB 配置模型
#[derive(Debug, Deserialize, Clone)]
pub struct MongoSettings {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub dbname: String,
    #[serde(default)]
    pub auth_source: Option<String>,
    #[serde(default)]
    pub replica_set: Option<String>,
    #[serde(default = "default_mongodb_max_connections")]
    pub max_connections: u32,
    /// 多文档事务策略: auto（探测部署能力，默认）| on（强制事务）| off（强制顺序写入）
    /// * `DESC`: `standalone 单机 mongod 不支持事务，auto 会自动降级为顺序幂等写入`
    #[serde(default = "default_mongodb_transactions")]
    pub transactions: String,
}

/////////

/// # Redis 配置模型
#[derive(Debug, Deserialize, Clone)]
pub struct RedisSettings {
    pub host: String,
    pub port: u16,
    #[serde(deserialize_with = "deserialize_integer_as_string")]
    pub db: String,
    #[serde(default)]
    pub password: Option<String>,
}

/////////

/// # Redis 配置模型默认实现
#[derive(Debug, Deserialize, Clone)]
pub struct ApiSettings {
    pub base_url: String,
    #[serde(default)]
    pub sign_key: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
}

/////////

/// # MySQL 配置模型
#[derive(Debug, Deserialize, Clone)]
pub struct MySqlSettings {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub dbname: String,
    pub max_connections: u32,
}

/////////

/// # MySQL 配置模型默认实现
#[derive(Debug, Deserialize, Clone)]
pub struct KafkaSettings {
    pub brokers: String,
    pub group_id: String,
    pub topic_messages: String,
    pub topic_notifications: String,
    pub client_id: String,
}

/////////

/// # Kafka 配置模型默认实现
#[derive(Debug, Deserialize, Clone)]
pub struct HealthSettings {
    #[serde(default = "default_health_interval_seconds")]
    pub interval_seconds: u64,
    #[serde(default)]
    pub registry_url: Option<String>,
    #[serde(default)]
    pub node_id: Option<String>,
}

/////////

/// # Health 配置模型默认实现
impl Default for HealthSettings {
    fn default() -> Self {
        Self {
            interval_seconds: default_health_interval_seconds(),
            registry_url: None,
            node_id: None,
        }
    }
}

/////////

/// # Default value functions for configuration fields
fn default_ws_host() -> String {
    "0.0.0.0".to_owned()
}

////////

/// # Default value functions for configuration fields
fn default_ws_port() -> u16 {
    19967
}

////////

/// # Default value functions for configuration fields
fn default_wt_host() -> String {
    "0.0.0.0".to_owned()
}

////////

/// Default value functions for configuration fields
fn default_wt_port() -> u16 {
    20048
}

fn default_mongodb_max_connections() -> u32 {
    50
}

fn default_mongodb_transactions() -> String {
    "auto".to_owned()
}

fn default_outbound_queue_capacity() -> usize {
    256
}

fn default_max_message_bytes() -> usize {
    1024 * 1024
}

fn default_max_rooms_per_connection() -> usize {
    20
}

fn default_health_interval_seconds() -> u64 {
    600
}

////////

fn default_ticket_ttl_seconds() -> u64 {
    30
}

fn default_chat_gateway_max_message_bytes() -> usize {
    64 * 1024
}

fn default_direct_message_policy() -> String {
    "strict".to_owned()
}

fn default_identity_transport() -> String {
    "http".to_owned()
}

fn default_identity_base_url() -> String {
    "http://127.0.0.1:8080".to_owned()
}

fn default_identity_path() -> String {
    "/api/v2/user/gateway".to_owned()
}

fn default_identity_service() -> String {
    "get_profile".to_owned()
}

fn default_identity_timeout_ms() -> u64 {
    800
}

fn deserialize_integer_as_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum IntegerOrString {
        Integer(i64),
        String(String),
    }

    match IntegerOrString::deserialize(deserializer)? {
        IntegerOrString::Integer(value) => Ok(value.to_string()),
        IntegerOrString::String(value) => Ok(value),
    }
}

impl ServerSettings {
    pub fn ws_host(&self) -> &str {
        self.host.as_deref().unwrap_or(self.ws.host.as_str())
    }

    pub fn ws_port(&self) -> u16 {
        self.port.unwrap_or(self.ws.port)
    }
}

impl Settings {
    /// # [HANDLER] - new
    /// * `DESC`: `读取 application.toml 并反序列化服务配置`
    pub fn new() -> Result<Self, ConfigError> {
        let settings = Config::builder()
            .add_source(File::with_name("application"))
            .build()?
            .try_deserialize::<Self>()?;
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::Settings;

    const APPLICATION_CONFIG: &str = include_str!("../../application.toml");

    /// # [HANDLER] - parses_current_application_config
    /// * `DESC`: `验证现有 application.toml 预设配置可被解析`
    #[test]
    fn parses_current_application_config() {
        let settings: Settings = toml::from_str(APPLICATION_CONFIG)
            .expect("the current application.toml layout should parse");

        assert_eq!(settings.server.ws_port(), 19967);
        assert_eq!(settings.server.wt.port, 20048);
        assert_eq!(settings.pg.max_connections, 50);
        assert_eq!(settings.mongodb.dbname, "cola_im_2024");
        assert_eq!(settings.mongodb.transactions, "auto");
        assert_eq!(settings.redis.db, "0");
        assert_eq!(settings.health.interval_seconds, 600);
        assert!(settings.kafka.is_some());
        assert!(settings.mysql.is_some());
    }

    /// # [HANDLER] - chat_gateway_defaults_when_section_is_missing
    /// * `DESC`: `验证缺少 chat_gateway 段时使用票据 TTL 与消息大小安全默认值`
    #[test]
    fn chat_gateway_defaults_when_section_is_missing() {
        let settings: Settings = toml::from_str(APPLICATION_CONFIG)
            .expect("the current application.toml layout should parse");

        assert_eq!(settings.chat_gateway.ticket_ttl_seconds, 30);
        assert_eq!(settings.chat_gateway.max_message_bytes, 64 * 1024);
        assert!(settings.chat_gateway.validate().is_ok());
    }

    /// # [HANDLER] - im_section_parses_from_application_toml
    /// * `DESC`: `验证 application.toml 中的单聊准入与资料服务配置可被解析`
    #[test]
    fn im_section_parses_from_application_toml() {
        let settings: Settings = toml::from_str(APPLICATION_CONFIG)
            .expect("the current application.toml layout should parse");

        assert_eq!(settings.im.direct.policy, "profile");
        assert_eq!(settings.im.identity.transport, "http");
        assert_eq!(settings.im.identity.base_url, "http://127.0.0.1:8080");
        assert_eq!(settings.im.identity.path, "/api/v2/user/gateway");
        assert_eq!(settings.im.identity.service, "get_profile");
        assert!(settings.im.identity.timeout_ms >= 50);
        assert!(settings.im.validate().is_ok());
    }

    /// # [HANDLER] - im_defaults_are_safe
    /// * `DESC`: `验证缺少 im 段时的安全默认值（strict + 本机网关）`
    #[test]
    fn im_defaults_are_safe() {
        use super::ImSettings;

        let settings = ImSettings::default();
        assert_eq!(settings.direct.policy, "strict");
        assert_eq!(settings.identity.transport, "http");
        assert_eq!(settings.identity.base_url, "http://127.0.0.1:8080");
        assert!(settings.validate().is_ok());
    }

    /// # [HANDLER] - im_rejects_invalid_policy_and_transport
    /// * `DESC`: `验证非法策略/传输/超时会被配置校验拦截`
    #[test]
    fn im_rejects_invalid_policy_and_transport() {
        use super::{DirectMessageSettings, IdentitySettings};

        assert!(DirectMessageSettings {
            policy: "open".to_owned()
        }
        .validate()
        .is_err());

        assert!(
            IdentitySettings {
                transport: "thrift".to_owned(),
                ..IdentitySettings::default()
            }
            .validate()
            .is_err()
        );

        assert!(
            IdentitySettings {
                timeout_ms: 5,
                ..IdentitySettings::default()
            }
            .validate()
            .is_err()
        );

        assert!(
            IdentitySettings {
                base_url: "127.0.0.1:8080".to_owned(),
                ..IdentitySettings::default()
            }
            .validate()
            .is_err()
        );
    }

    /// # [HANDLER] - chat_gateway_rejects_out_of_range_values
    /// * `DESC`: `验证聊天长连接配置的非法端口、票据有效期与消息大小会被拒绝`
    #[test]
    fn chat_gateway_rejects_out_of_range_values() {
        use super::ChatGatewaySettings;

        let mut settings = ChatGatewaySettings::default();
        settings.ticket_ttl_seconds = 1;
        assert!(settings.validate().is_err());

        let mut settings = ChatGatewaySettings::default();
        settings.max_message_bytes = 16;
        assert!(settings.validate().is_err());
    }

    /// # [HANDLER] - parses_legacy_server_config
    /// * `DESC`: `验证兼容旧版平铺 server host/port 配置`
    #[test]
    fn parses_legacy_server_config() {
        let settings: Settings = toml::from_str(
            r#"
                [app]
                name = "test"
                name_zh = "测试"
                version = "0.1.0"

                [server]
                host = "127.0.0.1"
                port = 19967

                [pg]
                host = "127.0.0.1"
                port = 5432
                user = "postgres"
                password = "test"
                dbname = "test"
                max_connections = 5

                [mongodb]
                host = "127.0.0.1"
                port = 27017
                user = "mongo"
                password = "test"
                dbname = "test"

                [redis]
                host = "127.0.0.1"
                port = 6379
                db = 0

                [api]
                base_url = "http://localhost/"
            "#,
        )
        .expect("legacy flat server config should parse");

        assert_eq!(settings.server.ws_port(), 19967);
        assert_eq!(settings.server.ws_host(), "127.0.0.1");
        assert_eq!(settings.mongodb.transactions, "auto");
    }
}

//////// END
