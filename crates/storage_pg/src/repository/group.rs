// crates/storage_pg/src/repository/group.rs -- PostgreSQL group membership adapter
// 2026/10/7 21:46 Created.

use app_core::ports::message::GroupMembershipRepository;
use async_trait::async_trait;
use sqlx::PgPool;

use super::log_sqlx_error;

#[derive(Clone)]
pub struct PgGroupMembershipRepository {
    pool: PgPool,
}

impl PgGroupMembershipRepository {
    /// # [HANDLER] - new
    /// * `DESC`: `创建 PostgreSQL 群成员关系仓储`
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl GroupMembershipRepository for PgGroupMembershipRepository {
    async fn list_member_account_ids(&self, group_id: &str) -> Result<Vec<u64>, String> {
        let member_ids = sqlx::query_scalar::<_, i64>(
            "SELECT account_id FROM im_2024.group_memberships WHERE group_id = $1 AND status = 1 ORDER BY account_id",
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| {
            log_sqlx_error("group.list_member_account_ids", &error);
            error.to_string()
        })?;

        let members: Vec<u64> = member_ids
            .into_iter()
            .map(|member_id| {
                u64::try_from(member_id)
                    .map_err(|_| "negative account id in group membership".to_owned())
            })
            .collect::<Result<_, _>>()?;

        tracing::info!(
            target: "storage_pg::group",
            group_id = group_id,
            member_count = members.len(),
            "[👥 PG GROUP] list_member_account_ids 查询完成；成员数为 0 时群消息没有收件人"
        );
        Ok(members)
    }

    async fn is_member(&self, group_id: &str, account_id: u64) -> Result<bool, String> {
        let account_id = i64::try_from(account_id)
            .map_err(|_| "account id exceeds PostgreSQL bigint".to_owned())?;
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM im_2024.group_memberships WHERE group_id = $1 AND account_id = $2 AND status = 1)",
        )
        .bind(group_id)
        .bind(account_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| {
            log_sqlx_error("group.is_member", &error);
            error.to_string()
        })
    }
}

////////
