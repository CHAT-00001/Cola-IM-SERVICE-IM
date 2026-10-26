// app_config/src/config.rs -- 应用配置 - 全局配置
// 2025-12-10 14:10

////////

use serde::Deserialize;

////////

pub mod config_loader;

////////

/// ---------------------------
/// APP - 应用全局配置
/// ---------------------------
#[derive(Debug, Clone, Deserialize)]
pub struct App {
    pub name: String,          // 应用名称
    pub debug: bool,           // DEBUG模式开关
    pub signature: String,     // 签名
    pub enable_protobuf: bool, // 二进制开关
}

////////

/// ---------------------------
/// API - 接口服务
/// ---------------------------
#[derive(Debug, Clone, Deserialize)]
pub struct Api {
    pub host: String, // 主机
    pub port: u16,    // 端口
}

////////

/// ---------------------------
/// WS - 长连接
/// ---------------------------
#[derive(Debug, Clone, Deserialize)]
pub struct Ws {
    pub host: String, // 主机
    pub port: u16,    // 端口
}

////////

/// ---------------------------
/// Health - 健康中心
/// ---------------------------
#[derive(Debug, Clone, Deserialize)]
pub struct Health {
    pub host: String, // 主机
    pub port: u16,    // 端口
}

////////

/// ---------------------------
/// Gateway - 全局网关
/// ---------------------------
#[derive(Debug, Clone, Deserialize)]
pub struct Gateway {
    pub host: String, // 主机
    pub port: u16,    // 端口
}

////////

/// ---------------------------
/// PostgreSQL 配置
/// ---------------------------
#[derive(Debug, Clone, Deserialize)]
pub struct Pg {
    pub host: String,                 // 主机
    pub port: u16,                    // 端口
    pub username: String,             // 用户名
    pub password: String,             // 密码
    pub database: String,             // 数据库名称
    pub max_connections: Option<u32>, // 最大连接数量
}

////////

/// ---------------------------
/// Redis 配置
/// ---------------------------
#[derive(Debug, Clone, Deserialize)]
pub struct Redis {
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub password: Option<String>,
    pub db: Option<u32>,
}

////////

/// ---------------------------
/// MongoDB 配置
/// ---------------------------
#[derive(Debug, Clone, Deserialize)]
pub struct Mongodb {
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub password: Option<String>,
    pub database: String,
    #[serde(default)]
    pub auth_source: Option<String>,
    #[serde(default)]
    pub max_connections: Option<u32>,
    #[serde(default)]
    pub replica_set: Option<String>,
}

////////

/// ---------------------------
/// 全局 AppConfig
/// ---------------------------
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub app: App,
    pub api: Api,
    pub ws: Ws,
    pub health: Health,
    pub gateway: Gateway,
    pub pg: Pg,
    pub redis: Redis,
    pub mongodb: Mongodb,
}

//////// END
