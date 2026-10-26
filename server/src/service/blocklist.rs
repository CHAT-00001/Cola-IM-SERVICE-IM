// server/src/service/blocklist.rs -- Placeholder for the bidirectional chat blocklist gateway
// 2026/10/11 17:17 Created.

////////

use app_core::ports::message::DirectConversationBlocklist;
use async_trait::async_trait;

////////

/// # [HANDLER] - AllowAllBlocklist
/// * `DESC`: `明确标记黑名单尚未接入并暂时允许所有关系`
#[derive(Clone, Copy, Default)]
pub struct AllowAllBlocklist;

#[async_trait]
impl DirectConversationBlocklist for AllowAllBlocklist {
    async fn is_blocked(
        &self,
        _blocker_account_id: u64,
        _blocked_account_id: u64,
    ) -> Result<bool, String> {
        Ok(false)
    }
}

//////// END
