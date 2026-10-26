// server/src/service/identity/mod.rs -- 用户资料 - 跨进程调用（HTTP / gRPC）
// 2026-10-26 Created.

////////

pub mod grpc; // gRPC 传输（预留）
pub mod http; // HTTP 传输

////////

pub use grpc::GrpcUserIdentity;
pub use http::HttpUserIdentity;

use app_core::models::user::UserProfile;
use app_core::ports::identity::UserIdentityRepository;
use async_trait::async_trait;

use crate::config::IdentitySettings;

////////

/// # [ADAPTER] - 用户资料传输通道
/// * `DESC`: `按配置在 HTTP 与 gRPC 之间切换；业务层只依赖 UserIdentityRepository`
#[derive(Clone)]
pub enum IdentitySource {
    Http(HttpUserIdentity),
    Grpc(GrpcUserIdentity),
}

////////

impl IdentitySource {
    /// # [ADAPTER] - from_settings
    /// * `DESC`: `读取 im.identity.transport 装配传输实现`
    pub fn from_settings(settings: &IdentitySettings) -> Result<Self, String> {
        match settings.transport.trim().to_ascii_lowercase().as_str() {
            "http" => Ok(Self::Http(HttpUserIdentity::new(settings))),
            "grpc" => Ok(Self::Grpc(GrpcUserIdentity::new(settings))),
            other => Err(format!(
                "im.identity.transport 非法: '{other}'（可选 http / grpc）"
            )),
        }
    }
}

////////

#[async_trait]
impl UserIdentityRepository for IdentitySource {
    async fn fetch_profile(&self, account_id: u64) -> Result<Option<UserProfile>, String> {
        match self {
            Self::Http(client) => client.fetch_profile(account_id).await,
            Self::Grpc(client) => client.fetch_profile(account_id).await,
        }
    }

    fn transport(&self) -> &'static str {
        match self {
            Self::Http(client) => client.transport(),
            Self::Grpc(client) => client.transport(),
        }
    }
}

////////

#[cfg(test)]
mod tests {
    use super::IdentitySource;
    use crate::config::IdentitySettings;

    /// # [HANDLER] - http_transport_is_default
    /// * `DESC`: `验证默认配置装配 HTTP 传输`
    #[test]
    fn http_transport_is_default() {
        use app_core::ports::identity::UserIdentityRepository;

        let source = IdentitySource::from_settings(&IdentitySettings::default())
            .expect("default transport should be valid");
        assert_eq!(source.transport(), "http");
    }

    /// # [HANDLER] - grpc_transport_can_be_selected
    /// * `DESC`: `验证 transport=grpc 可切换到预留实现`
    #[test]
    fn grpc_transport_can_be_selected() {
        use app_core::ports::identity::UserIdentityRepository;

        let settings = IdentitySettings {
            transport: "GRPC".to_owned(),
            ..IdentitySettings::default()
        };
        let source = IdentitySource::from_settings(&settings).expect("grpc transport should build");
        assert_eq!(source.transport(), "grpc");
    }

    /// # [HANDLER] - unknown_transport_is_rejected
    /// * `DESC`: `验证未知传输通道会被拒绝`
    #[test]
    fn unknown_transport_is_rejected() {
        let settings = IdentitySettings {
            transport: "thrift".to_owned(),
            ..IdentitySettings::default()
        };
        assert!(IdentitySource::from_settings(&settings).is_err());
    }
}

//////// END
