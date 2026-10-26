// server/src/chat/ticket.rs -- WebSocket 一次性握手票据存储
// 2026/10/25 23:41 Created.

////////

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::chat::model::AuthenticatedSession;

////////

/// # [HANDLER] - TicketStore
/// * `DESC`: `内存中的一次性 WS 握手票据表，签发时绑定已认证的账号与设备，消费后立即失效`
pub struct TicketStore {
    ttl: Duration,
    entries: Mutex<HashMap<String, TicketEntry>>,
}

struct TicketEntry {
    session: AuthenticatedSession,
    expires_at: Instant,
}

////////

impl TicketStore {
    /// # [HANDLER] - new
    /// * `DESC`: `创建票据表，票据有效期由配置决定`
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// # [HANDLER] - issue
    /// * `DESC`: `为已认证会话签发随机票据，并顺带清理已过期的票据`
    pub fn issue(&self, session: &AuthenticatedSession) -> String {
        // 两个 UUID v4 共 244 位随机熵，由操作系统随机源提供，避免新增依赖。
        let ticket = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let now = Instant::now();
        let mut entries = self.entries.lock().expect("ticket store mutex poisoned");
        entries.retain(|_, entry| entry.expires_at > now);
        entries.insert(
            ticket.clone(),
            TicketEntry {
                session: AuthenticatedSession {
                    account_id: session.account_id,
                    device_id: session.device_id.clone(),
                    connection_id: String::new(),
                },
                expires_at: now + self.ttl,
            },
        );
        ticket
    }

    /// # [HANDLER] - consume
    /// * `DESC`: `原子地取出并删除票据，票据只能成功消费一次且过期后无效`
    pub fn consume(&self, ticket: &str) -> Option<AuthenticatedSession> {
        let entry = self
            .entries
            .lock()
            .expect("ticket store mutex poisoned")
            .remove(ticket)?;
        if entry.expires_at <= Instant::now() {
            return None;
        }
        Some(entry.session)
    }

    /// # [HANDLER] - ttl_seconds
    /// * `DESC`: `返回票据有效期（秒），供签发响应告知客户端`
    pub fn ttl_seconds(&self) -> u64 {
        self.ttl.as_secs()
    }
}

////////

#[cfg(test)]
mod tests {
    use std::thread::sleep;
    use std::time::Duration;

    use super::TicketStore;
    use crate::chat::model::AuthenticatedSession;

    fn session() -> AuthenticatedSession {
        AuthenticatedSession {
            account_id: 42,
            device_id: "phone".to_owned(),
            connection_id: String::new(),
        }
    }

    /// # [HANDLER] - ticket_is_consumed_once
    /// * `DESC`: `验证票据只能消费一次，第二次消费必须失败`
    #[test]
    fn ticket_is_consumed_once() {
        let store = TicketStore::new(Duration::from_secs(30));
        let ticket = store.issue(&session());

        let consumed = store.consume(&ticket).expect("first consume should succeed");
        assert_eq!(consumed.account_id, 42);
        assert_eq!(consumed.device_id, "phone");
        assert!(store.consume(&ticket).is_none());
    }

    /// # [HANDLER] - forged_ticket_is_rejected
    /// * `DESC`: `验证未签发的票据不会被接受`
    #[test]
    fn forged_ticket_is_rejected() {
        let store = TicketStore::new(Duration::from_secs(30));
        let _ = store.issue(&session());

        assert!(store.consume("forged-ticket").is_none());
        assert!(store.consume("").is_none());
    }

    /// # [HANDLER] - expired_ticket_is_rejected
    /// * `DESC`: `验证超过有效期的票据无法消费`
    #[test]
    fn expired_ticket_is_rejected() {
        let store = TicketStore::new(Duration::from_millis(20));
        let ticket = store.issue(&session());

        sleep(Duration::from_millis(60));
        assert!(store.consume(&ticket).is_none());
    }

    /// # [HANDLER] - issued_tickets_are_unique
    /// * `DESC`: `验证连续签发的票据互不相同`
    #[test]
    fn issued_tickets_are_unique() {
        let store = TicketStore::new(Duration::from_secs(30));
        let first = store.issue(&session());
        let second = store.issue(&session());

        assert_ne!(first, second);
        assert!(first.len() >= 64);
    }
}

//////// END
