// crates/storage_pg/src/repository/contact.rs -- PostgreSQL contact relationship adapter
// 2026/10/7 21:46 Created.

use async_trait::async_trait;
use sqlx::PgPool;

use app_core::models::user::UserProfile;
use app_core::ports::message::ContactRelationshipRepository;

use super::log_sqlx_error;

#[derive(Clone)]
pub struct PgContactRelationshipRepository {
    pool: PgPool,
}

impl PgContactRelationshipRepository {
    /// # [HANDLER] - new
    /// * `DESC`: `创建 PostgreSQL 联系人关系仓储`
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ContactRelationshipRepository for PgContactRelationshipRepository {
    async fn can_message(
        &self,
        sender_account_id: u64,
        recipient_account_id: u64,
    ) -> Result<bool, String> {
        let sender_id = i64::try_from(sender_account_id)
            .map_err(|_| "sender account id is outside PostgreSQL bigint range".to_owned())?;
        let recipient_id = i64::try_from(recipient_account_id)
            .map_err(|_| "recipient account id is outside PostgreSQL bigint range".to_owned())?;

        let allowed = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM im_2024.contact_list
                WHERE user_id = $1
                  AND target_id = $2
                  AND status = 1
                  AND is_deleted = FALSE
            )
            "#,
        )
        .bind(sender_id)
        .bind(recipient_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| {
            log_sqlx_error("contact.can_message", &error);
            error.to_string()
        })?;

        tracing::info!(
            target: "storage_pg::contact",
            sender_account_id = sender_account_id,
            recipient_account_id = recipient_account_id,
            allowed = allowed,
            "[📇 PG CONTACT] can_message 查询结果；allowed=false 时单聊会在落库前被拒绝（需 contact_list 中存在有效联系人关系）"
        );
        Ok(allowed)
    }

    async fn upsert_contact_profile(&self, profile: &UserProfile) -> Result<(), String> {
        let account_id = i64::try_from(profile.account_id)
            .map_err(|_| "profile account id is outside PostgreSQL bigint range".to_owned())?;
        // 联系人档案表没有单独的账号列：约定 `contact_profile.id` 直接保存主后端账号 ID，
        // 与 `contact_list.user_id / target_id` 使用同一套 ID，使 `can_message` 的关联天然成立。
        let name = if profile.nickname.trim().is_empty() {
            format!("user-{account_id}")
        } else {
            profile.nickname.trim().to_owned()
        };

        sqlx::query(
            r#"
            INSERT INTO im_2024.contact_profile
                (id, status, name, avatar_url, description, gender, contact_type)
            VALUES ($1, $2, $3, $4, $5, $6, 0)
            ON CONFLICT (id) DO UPDATE
            SET status      = EXCLUDED.status,
                name        = EXCLUDED.name,
                avatar_url  = EXCLUDED.avatar_url,
                description = EXCLUDED.description,
                gender      = EXCLUDED.gender,
                updated_at  = NOW()
            "#,
        )
        .bind(account_id)
        .bind(profile.status.unwrap_or(1))
        .bind(name)
        .bind(profile.avatar_url.as_deref())
        .bind(profile.signature.as_deref())
        .bind(profile.gender)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            log_sqlx_error("contact.upsert_profile", &error);
            error.to_string()
        })?;

        tracing::info!(
            target: "storage_pg::contact",
            account_id = profile.account_id,
            "[📇 PG CONTACT] 远端用户资料已写入 contact_profile"
        );
        Ok(())
    }

    async fn ensure_contact_relation(
        &self,
        owner_account_id: u64,
        target_account_id: u64,
    ) -> Result<(), String> {
        let owner_id = i64::try_from(owner_account_id)
            .map_err(|_| "owner account id is outside PostgreSQL bigint range".to_owned())?;
        let target_id = i64::try_from(target_account_id)
            .map_err(|_| "target account id is outside PostgreSQL bigint range".to_owned())?;

        let mut transaction = self.pool.begin().await.map_err(|error| {
            log_sqlx_error("contact.ensure_relation.begin", &error);
            error.to_string()
        })?;

        // 0. contact_list 只有代理主键，没有 (user_id, target_id) 唯一索引：
        //    用事务级 advisory lock 把同一对 (owner, target) 串行化，避免并发补建产生重复行。
        sqlx::query(
            r#"
            SELECT pg_advisory_xact_lock(
                hashtextextended('im.contact_list:' || $1::text || ':' || $2::text, 0)
            )
            "#,
        )
        .bind(owner_id)
        .bind(target_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            log_sqlx_error("contact.ensure_relation.lock", &error);
            error.to_string()
        })?;

        // 1. 关系不存在时补建一条"仅发起方可见"的联系人关系，不替对方伪造通讯录条目。
        sqlx::query(
            r#"
            INSERT INTO im_2024.contact_list
                (status, user_id, target_id, contact_type, added_at, sync_at)
            SELECT 1, $1, $2, 0, NOW(), NOW()
            WHERE NOT EXISTS (
                SELECT 1
                FROM im_2024.contact_list
                WHERE user_id = $1
                  AND target_id = $2
            )
            "#,
        )
        .bind(owner_id)
        .bind(target_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            log_sqlx_error("contact.ensure_relation.insert", &error);
            error.to_string()
        })?;

        // 2. 关系被软删或失效时恢复为可用状态。
        sqlx::query(
            r#"
            UPDATE im_2024.contact_list
            SET status     = 1,
                is_deleted = FALSE,
                deleted_at = NULL,
                sync_at    = NOW()
            WHERE user_id = $1
              AND target_id = $2
              AND (status <> 1 OR is_deleted IS DISTINCT FROM FALSE)
            "#,
        )
        .bind(owner_id)
        .bind(target_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            log_sqlx_error("contact.ensure_relation.restore", &error);
            error.to_string()
        })?;

        transaction.commit().await.map_err(|error| {
            log_sqlx_error("contact.ensure_relation.commit", &error);
            error.to_string()
        })?;

        tracing::info!(
            target: "storage_pg::contact",
            owner_account_id = owner_account_id,
            target_account_id = target_account_id,
            "[📇 PG CONTACT] 联系人关系已补建/恢复"
        );
        Ok(())
    }
}

////////
