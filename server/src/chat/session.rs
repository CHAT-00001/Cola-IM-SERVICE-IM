// server/src/chat/session.rs -- Direct chat session application service
// 2026/10/11 17:17 Created.

////////

use app_core::models::chat::{ChatSessionListItem, ChatSessionView};
use app_core::ports::message::{DirectChatSessionRepository, DirectConversationBlocklist};

use crate::chat::port::ChatError;

////////

#[derive(Clone)]
pub struct DirectChatSessionService<R, B> {
    repository: R,
    blocklist: B,
}

impl<R, B> DirectChatSessionService<R, B>
where
    R: DirectChatSessionRepository,
    B: DirectConversationBlocklist,
{
    /// # [HANDLER] - new
    /// * `DESC`: `创建单聊会话编排服务并注入会话仓储与黑名单端口`
    pub fn new(repository: R, blocklist: B) -> Self {
        Self {
            repository,
            blocklist,
        }
    }

    /// # [HANDLER] - ensure_session
    /// * `DESC`: `双向检查黑名单后幂等创建或返回单聊会话`
    pub async fn ensure_session(
        &self,
        owner_account_id: u64,
        peer_account_id: u64,
    ) -> Result<ChatSessionView, ChatError> {
        if owner_account_id == 0 || peer_account_id == 0 || owner_account_id == peer_account_id {
            return Err(ChatError::InvalidMessage(
                "direct session requires two distinct positive account ids".to_owned(),
            ));
        }

        for (blocker, blocked) in [
            (owner_account_id, peer_account_id),
            (peer_account_id, owner_account_id),
        ] {
            if self
                .blocklist
                .is_blocked(blocker, blocked)
                .await
                .map_err(|error| ChatError::Persistence(format!("blocklist check: {error}")))?
            {
                return Err(ChatError::Unauthorized);
            }
        }

        let session = self
            .repository
            .ensure_direct_session(owner_account_id, peer_account_id)
            .await
            .map_err(|error| ChatError::Persistence(format!("chat session repository: {error}")))?;
        if session.status != 1 || session.member_state != 1 {
            return Err(ChatError::Unauthorized);
        }
        Ok(session)
    }

    /// # [HANDLER] - list_sessions
    /// * `DESC`: `返回指定账号参与的聊天会话列表`
    pub async fn list_sessions(
        &self,
        account_id: u64,
    ) -> Result<Vec<ChatSessionListItem>, ChatError> {
        if account_id == 0 {
            return Err(ChatError::InvalidMessage(
                "account id must be positive".to_owned(),
            ));
        }
        self.repository
            .list_sessions(account_id)
            .await
            .map_err(|error| ChatError::Persistence(format!("chat session repository: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use app_core::models::chat::{ChatSessionListItem, ChatSessionView};
    use app_core::ports::message::{DirectChatSessionRepository, DirectConversationBlocklist};
    use async_trait::async_trait;

    use super::DirectChatSessionService;
    use crate::chat::port::ChatError;

    #[derive(Clone, Default)]
    struct FakeSessions {
        calls: Arc<Mutex<Vec<(u64, u64)>>>,
    }

    #[async_trait]
    impl DirectChatSessionRepository for FakeSessions {
        async fn ensure_direct_session(
            &self,
            owner: u64,
            peer: u64,
        ) -> Result<ChatSessionView, String> {
            self.calls.lock().unwrap().push((owner, peer));
            Ok(ChatSessionView {
                session_id: "uuid-v4-session".to_owned(),
                channel: 1,
                status: 1,
                pinned: false,
                last_message_summary: None,
                notification_settings: serde_json::json!({}),
                member_state: 1,
            })
        }

        async fn update_last_message(
            &self,
            _session_id: &str,
            _message_id: &str,
            _summary: &str,
            _sent_at: chrono::DateTime<chrono::Utc>,
        ) -> Result<(), String> {
            Ok(())
        }

        async fn list_sessions(
            &self,
            _account_id: u64,
        ) -> Result<Vec<ChatSessionListItem>, String> {
            Ok(Vec::new())
        }

        async fn peer_account_id_in_session(
            &self,
            _session_id: &str,
            _account_id: u64,
        ) -> Result<Option<u64>, String> {
            Ok(None)
        }
    }

    #[derive(Clone)]
    struct FakeBlocklist {
        blocked_pair: Option<(u64, u64)>,
        checks: Arc<Mutex<Vec<(u64, u64)>>>,
    }

    #[async_trait]
    impl DirectConversationBlocklist for FakeBlocklist {
        async fn is_blocked(&self, blocker: u64, blocked: u64) -> Result<bool, String> {
            self.checks.lock().unwrap().push((blocker, blocked));
            Ok(self.blocked_pair == Some((blocker, blocked)))
        }
    }

    /// # [HANDLER] - creates_session_after_checking_both_block_directions
    /// * `DESC`: `验证未命中黑名单时按双向顺序检查并获取单聊会话`
    #[tokio::test]
    async fn creates_session_after_checking_both_block_directions() {
        let repository = FakeSessions::default();
        let checks = Arc::new(Mutex::new(Vec::new()));
        let service = DirectChatSessionService::new(
            repository.clone(),
            FakeBlocklist {
                blocked_pair: None,
                checks: checks.clone(),
            },
        );

        let view = service.ensure_session(10, 20).await.unwrap();

        assert_eq!(view.session_id, "uuid-v4-session");
        assert_eq!(*checks.lock().unwrap(), vec![(10, 20), (20, 10)]);
        assert_eq!(*repository.calls.lock().unwrap(), vec![(10, 20)]);
    }

    /// # [HANDLER] - rejects_session_when_either_direction_is_blocked
    /// * `DESC`: `验证任意一方拉黑时拒绝创建且不触碰会话仓储`
    #[tokio::test]
    async fn rejects_session_when_either_direction_is_blocked() {
        let repository = FakeSessions::default();
        let checks = Arc::new(Mutex::new(Vec::new()));
        let service = DirectChatSessionService::new(
            repository.clone(),
            FakeBlocklist {
                blocked_pair: Some((20, 10)),
                checks: checks.clone(),
            },
        );

        assert!(matches!(
            service.ensure_session(10, 20).await,
            Err(ChatError::Unauthorized)
        ));
        assert_eq!(*checks.lock().unwrap(), vec![(10, 20), (20, 10)]);
        assert!(repository.calls.lock().unwrap().is_empty());
    }

    /// # [HANDLER] - list_sessions_rejects_zero_account
    /// * `DESC`: `验证账号 ID 非正时拒绝列出聊天会话`
    #[tokio::test]
    async fn list_sessions_rejects_zero_account() {
        let service = DirectChatSessionService::new(
            FakeSessions::default(),
            FakeBlocklist {
                blocked_pair: None,
                checks: Arc::new(Mutex::new(Vec::new())),
            },
        );

        assert!(matches!(
            service.list_sessions(0).await,
            Err(ChatError::InvalidMessage(_))
        ));
    }
}

//////// END
