// app_core/src/ports/identity.rs -- 端口 - 用户资料（跨进程调用主后端）
// 2026-10-26 Created.

////////

use crate::models::user::UserProfile;
use async_trait::async_trait;

////////

/// # [PORT] - 用户资料远端仓储
/// * `DESC`: `跨进程拉取主后端用户资料；HTTP 与 gRPC 两种传输实现同一端口，
///   业务层只依赖本 trait，切换传输只需替换 `server/src/service/identity` 下的实现。`
#[async_trait]
pub trait UserIdentityRepository: Send + Sync {
    /// 行为 1：按账号拉取用户资料；账号不存在时返回 `Ok(None)`
    async fn fetch_profile(&self, account_id: u64) -> Result<Option<UserProfile>, String>;

    /// 行为 2：当前传输通道名称（`http` / `grpc`），仅用于日志与排障
    fn transport(&self) -> &'static str;
}

//////// END
