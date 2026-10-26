// crates/storage_pg/src/repository/chat_session.rs -- PostgreSQL direct chat session repository
// 2026/10/11 17:17 Created.

////////

use app_core::models::chat::{ChatSessionListItem, ChatSessionView};
use app_core::ports::message::DirectChatSessionRepository;
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use super::log_sqlx_error;

////////

#[derive(Clone)]
pub struct PgDirectChatSessionRepository {
    pool: PgPool,
}

impl PgDirectChatSessionRepository {
    /// # [HANDLER] - new
    /// * `DESC`: `创建 PostgreSQL 单聊会话仓储`
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DirectChatSessionRepository for PgDirectChatSessionRepository {
    async fn ensure_direct_session(
        &self,
        owner_account_id: u64,
        peer_account_id: u64,
    ) -> Result<ChatSessionView, String> {
        let owner = i64::try_from(owner_account_id)
            .map_err(|_| "owner account id exceeds PostgreSQL bigint".to_owned())?;
        let peer = i64::try_from(peer_account_id)
            .map_err(|_| "peer account id exceeds PostgreSQL bigint".to_owned())?;
        if owner <= 0 || peer <= 0 || owner == peer {
            return Err(
                "direct session participants must be distinct positive account ids".to_owned(),
            );
        }

        let (user_low, user_high) = if owner < peer {
            (owner, peer)
        } else {
            (peer, owner)
        };
        let mut transaction = self.pool.begin().await.map_err(|error| {
            log_sqlx_error("chat_session.begin_transaction", &error);
            error.to_string()
        })?;
        sqlx::query(
            r#"
            INSERT INTO im_2024.chat_sessions (session_id, channel, user_low, user_high)
            VALUES ($1, 1, $2, $3)
            ON CONFLICT (user_low, user_high) DO NOTHING
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(user_low)
        .bind(user_high)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            log_sqlx_error("chat_session.insert_session", &error);
            error.to_string()
        })?;

        let session_id: Uuid = sqlx::query_scalar(
            "SELECT session_id FROM im_2024.chat_sessions WHERE user_low = $1 AND user_high = $2 FOR UPDATE",
        )
        .bind(user_low)
        .bind(user_high)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| {
            log_sqlx_error("chat_session.select_session_for_update", &error);
            error.to_string()
        })?;

        sqlx::query(
            r#"
            INSERT INTO im_2024.chat_session_members (session_id, account_id)
            VALUES ($1, $2), ($1, $3)
            ON CONFLICT (session_id, account_id) DO NOTHING
            "#,
        )
        .bind(session_id)
        .bind(user_low)
        .bind(user_high)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            log_sqlx_error("chat_session.insert_members", &error);
            error.to_string()
        })?;

        let view = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT s.session_id, s.channel, s.status, s.last_message_summary,
                   m.is_pinned, m.notification_settings, m.member_state
            FROM im_2024.chat_sessions AS s
            JOIN im_2024.chat_session_members AS m ON m.session_id = s.session_id
            WHERE s.session_id = $1 AND m.account_id = $2
            "#,
        )
        .bind(session_id)
        .bind(owner)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| {
            log_sqlx_error("chat_session.select_session_view", &error);
            error.to_string()
        })?;
        transaction
            .commit()
            .await
            .map_err(|error| {
                log_sqlx_error("chat_session.commit_transaction", &error);
                error.to_string()
            })?;

        tracing::info!(
            target: "storage_pg::chat_session",
            session_id = %view.session_id,
            owner_account_id = owner,
            peer_account_id = peer,
            status = view.status,
            member_state = view.member_state,
            "[💬 PG SESSION] ensure_direct_session 完成；status/member_state 不为 1 时单聊会被拒绝"
        );

        Ok(ChatSessionView {
            session_id: view.session_id.to_string(),
            channel: u8::try_from(view.channel)
                .map_err(|_| "invalid session channel".to_owned())?,
            status: view.status,
            pinned: view.is_pinned,
            last_message_summary: view.last_message_summary,
            notification_settings: view.notification_settings,
            member_state: view.member_state,
        })
    }

    async fn update_last_message(
        &self,
        session_id: &str,
        message_id: &str,
        summary: &str,
        sent_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), String> {
        let session_id = Uuid::parse_str(session_id).map_err(|error| error.to_string())?;
        let rows = sqlx::query(
            r#"
            UPDATE im_2024.chat_sessions
            SET last_message_id = $2, last_message_summary = $3, updated_at = $4
            WHERE session_id = $1 AND status = 1
            "#,
        )
        .bind(session_id)
        .bind(message_id)
        .bind(summary)
        .bind(sent_at)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            log_sqlx_error("chat_session.update_last_message", &error);
            error.to_string()
        })?
        .rows_affected();

        if rows == 0 {
            tracing::warn!(
                target: "storage_pg::chat_session",
                session_id = %session_id,
                message_id = message_id,
                "[💬 PG SESSION] update_last_message 影响 0 行：会话不存在或 status != 1，会话摘要未更新"
            );
        } else {
            tracing::info!(
                target: "storage_pg::chat_session",
                session_id = %session_id,
                message_id = message_id,
                rows = rows,
                "[💬 PG SESSION] update_last_message 完成"
            );
        }
        Ok(())
    }

    /// # [HANDLER] - list_sessions
    /// * `DESC`: `按账号列出其参与的聊天会话`
    async fn list_sessions(
        &self,
        account_id: u64,
    ) -> Result<Vec<ChatSessionListItem>, String> {
        let account = i64::try_from(account_id)
            .map_err(|_| "account id exceeds PostgreSQL bigint".to_owned())?;
        let rows = sqlx::query_as::<_, SessionListRow>(
            r#"
            SELECT s.session_id, s.channel, s.status, s.user_low, s.user_high,
                   s.last_message_id, s.last_message_summary, s.updated_at,
                   m.is_pinned, m.notification_settings, m.member_state, m.unread_count
            FROM im_2024.chat_sessions AS s
            JOIN im_2024.chat_session_members AS m ON m.session_id = s.session_id
            WHERE m.account_id = $1 AND m.member_state = 1 AND s.status = 1
            ORDER BY s.updated_at DESC
            LIMIT 200
            "#,
        )
        .bind(account)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| {
            log_sqlx_error("chat_session.list_sessions", &error);
            error.to_string()
        })?;

        Ok(rows
            .into_iter()
            .map(|row| ChatSessionListItem {
                session_id: row.session_id.to_string(),
                channel: row.channel,
                status: row.status,
                peer_account_id: if row.user_low == account {
                    row.user_high
                } else {
                    row.user_low
                },
                is_pinned: row.is_pinned,
                last_message_id: row.last_message_id,
                last_message_summary: row.last_message_summary,
                notification_settings: row.notification_settings,
                member_state: row.member_state,
                unread_count: row.unread_count,
                updated_at: Some(row.updated_at),
            })
            .collect())
    }

    /// # [HANDLER] - peer_account_id_in_session
    /// * `DESC`: `按会话 ID 与成员账号反查单聊对端（仅返回有效会话的另一方）`
    async fn peer_account_id_in_session(
        &self,
        session_id: &str,
        account_id: u64,
    ) -> Result<Option<u64>, String> {
        // session_id 不是 UUID（如客户端本地占位 room_id）时无法反查，交由调用方回退到 recipient_uid 校验。
        let Ok(session_id) = Uuid::parse_str(session_id) else {
            return Ok(None);
        };
        let account = i64::try_from(account_id)
            .map_err(|_| "account id exceeds PostgreSQL bigint".to_owned())?;

        let peer = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT CASE WHEN s.user_low = $2 THEN s.user_high ELSE s.user_low END
            FROM im_2024.chat_sessions AS s
            JOIN im_2024.chat_session_members AS m ON m.session_id = s.session_id
            WHERE s.session_id = $1
              AND m.account_id = $2
              AND s.channel = 1
              AND s.status = 1
              AND m.member_state = 1
            "#,
        )
        .bind(session_id)
        .bind(account)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| {
            log_sqlx_error("chat_session.peer_account_id_in_session", &error);
            error.to_string()
        })?;

        let peer_account_id = peer
            .map(|value| {
                u64::try_from(value)
                    .map_err(|_| "negative peer account id in chat session".to_owned())
            })
            .transpose()?;

        tracing::info!(
            target: "storage_pg::chat_session",
            session_id = %session_id,
            account_id = account,
            peer_account_id = ?peer_account_id,
            "[💬 PG SESSION] peer_account_id_in_session 完成；None 表示会话不存在/不属于该账号，调用方需回退到 recipient_uid 校验"
        );

        Ok(peer_account_id)
    }
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    session_id: Uuid,
    channel: i16,
    status: i16,
    last_message_summary: Option<String>,
    is_pinned: bool,
    notification_settings: serde_json::Value,
    member_state: i16,
}

////////

/// # [HANDLER] - SessionListRow
/// * `DESC`: `聊天列表查询结果的行映射结构`
#[derive(sqlx::FromRow)]
struct SessionListRow {
    session_id: Uuid,
    channel: i16,
    status: i16,
    user_low: i64,
    user_high: i64,
    last_message_id: Option<String>,
    last_message_summary: Option<String>,
    updated_at: chrono::DateTime<chrono::Utc>,
    is_pinned: bool,
    notification_settings: serde_json::Value,
    member_state: i16,
    unread_count: i64,
}

//////// END
