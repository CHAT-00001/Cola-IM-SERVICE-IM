// server/src/service/relationships.rs -- PostgreSQL relationship adapters for chat use cases
// 2026/10/7 21:46 Created.

////////

use app_core::ports::message::{ContactRelationshipRepository, GroupMembershipRepository};
use async_trait::async_trait;
use sqlx::PgPool;
use storage_pg::repository::contact::PgContactRelationshipRepository;
use storage_pg::repository::group::PgGroupMembershipRepository;

////////

#[derive(Clone)]
pub struct PgRelationshipGateway {
    inner: PgContactRelationshipRepository,
}

////////

#[derive(Clone)]
pub struct PgGroupGateway {
    inner: PgGroupMembershipRepository,
}

impl PgRelationshipGateway {
    /// # [HANDLER] - new
    /// * `DESC`: `构造 PostgreSQL 单聊联系人关系适配器`
    pub fn new(pool: PgPool) -> Self {
        Self {
            inner: PgContactRelationshipRepository::new(pool),
        }
    }
}

impl PgGroupGateway {
    /// # [HANDLER] - new
    /// * `DESC`: `构造 PostgreSQL 群成员关系适配器`
    pub fn new(pool: PgPool) -> Self {
        Self {
            inner: PgGroupMembershipRepository::new(pool),
        }
    }
}

#[async_trait]
impl ContactRelationshipRepository for PgRelationshipGateway {
    async fn can_message(
        &self,
        sender_account_id: u64,
        recipient_account_id: u64,
    ) -> Result<bool, String> {
        self.inner
            .can_message(sender_account_id, recipient_account_id)
            .await
    }

    async fn upsert_contact_profile(
        &self,
        profile: &app_core::models::user::UserProfile,
    ) -> Result<(), String> {
        self.inner.upsert_contact_profile(profile).await
    }

    async fn ensure_contact_relation(
        &self,
        owner_account_id: u64,
        target_account_id: u64,
    ) -> Result<(), String> {
        self.inner
            .ensure_contact_relation(owner_account_id, target_account_id)
            .await
    }
}

#[async_trait]
impl GroupMembershipRepository for PgGroupGateway {
    async fn list_member_account_ids(&self, group_id: &str) -> Result<Vec<u64>, String> {
        self.inner.list_member_account_ids(group_id).await
    }

    async fn is_member(&self, group_id: &str, account_id: u64) -> Result<bool, String> {
        self.inner.is_member(group_id, account_id).await
    }
}

//////// END
