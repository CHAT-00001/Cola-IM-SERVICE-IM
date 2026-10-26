// server/src/service/identity/grpc.rs -- 用户资料 - gRPC 传输实现（预留）
// 2026-10-26 Created.

////////

use app_core::models::user::UserProfile;
use app_core::ports::identity::UserIdentityRepository;
use async_trait::async_trait;

use crate::config::IdentitySettings;

////////

/// # [ADAPTER] - gRPC 用户资料客户端（预留接口）
/// * `DESC`: `与 HttpUserIdentity 实现同一端口，配置 im.identity.transport = "grpc" 即可切换。
///   当前只保留结构骨架与端点装配，正式接入时按下述步骤替换 fetch_profile 内部逻辑：`
///
/// ```text
/// 1. proto:  service UserProfileService { rpc GetProfile(GetProfileRequest) returns (GetProfileReply); }
/// 2. build:  tonic_build::compile_protos("proto/user_profile.proto")
/// 3. deps:   tonic = "0.13", prost = "0.13"
/// 4. main:   let channel = tonic::transport::Channel::from_shared(endpoint)?.connect().await?;
///            let mut client = UserProfileServiceClient::new(channel);
/// 5. fetch:  let reply = client.get_profile(GetProfileRequest { account_id }).await?;
///            Ok(reply.into_inner().profile.map(UserProfile::from))
/// ```
#[derive(Clone)]
pub struct GrpcUserIdentity {
    endpoint: String,
}

////////

impl GrpcUserIdentity {
    /// # [ADAPTER] - new
    /// * `DESC`: `装配 gRPC 端点（优先 grpc_base_url / grpc_port，缺省回落 127.0.0.1:50051）`
    pub fn new(settings: &IdentitySettings) -> Self {
        Self {
            endpoint: Self::endpoint(settings),
        }
    }

    /// # [ADAPTER] - endpoint
    /// * `DESC`: `计算 gRPC 端点字符串`
    pub fn endpoint(settings: &IdentitySettings) -> String {
        let base = settings
            .grpc_base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("http://127.0.0.1:50051")
            .trim_end_matches('/')
            .to_owned();

        match settings.grpc_port {
            Some(port) if port > 0 => format!("{base}:{port}"),
            _ => base,
        }
    }
}

////////

#[async_trait]
impl UserIdentityRepository for GrpcUserIdentity {
    async fn fetch_profile(&self, account_id: u64) -> Result<Option<UserProfile>, String> {
        Err(format!(
            "[📡 IDENTITY GRPC] gRPC 传输为预留实现（endpoint={}, account_id={}），请先接入 tonic 客户端或将 im.identity.transport 设为 http",
            self.endpoint, account_id
        ))
    }

    fn transport(&self) -> &'static str {
        "grpc"
    }
}

////////

#[cfg(test)]
mod tests {
    use super::GrpcUserIdentity;
    use crate::config::IdentitySettings;

    /// # [HANDLER] - endpoint_falls_back_to_default
    /// * `DESC`: `验证未配置 grpc 地址时使用默认端点`
    #[test]
    fn endpoint_falls_back_to_default() {
        let settings = IdentitySettings::default();
        assert_eq!(
            GrpcUserIdentity::endpoint(&settings),
            "http://127.0.0.1:50051"
        );
    }

    /// # [HANDLER] - endpoint_uses_configured_port
    /// * `DESC`: `验证配置了 grpc 端口时拼接端点`
    #[test]
    fn endpoint_uses_configured_port() {
        let settings = IdentitySettings {
            grpc_base_url: Some("http://10.0.0.8/".to_owned()),
            grpc_port: Some(9000),
            ..IdentitySettings::default()
        };
        assert_eq!(
            GrpcUserIdentity::endpoint(&settings),
            "http://10.0.0.8:9000"
        );
    }

    /// # [HANDLER] - reserved_transport_reports_unimplemented
    /// * `DESC`: `验证未接入时调用会返回明确错误而不是 panic`
    #[tokio::test]
    async fn reserved_transport_reports_unimplemented() {
        use app_core::ports::identity::UserIdentityRepository;

        let client = GrpcUserIdentity::new(&IdentitySettings::default());
        let error = client
            .fetch_profile(1000008801)
            .await
            .expect_err("reserved transport must fail loudly");
        assert!(error.contains("预留实现"), "unexpected error: {error}");
        assert_eq!(client.transport(), "grpc");
    }
}

//////// END
