// server/src/service/contact_sync.rs -- 服务 - 单聊准入与联系人模型同步
// 2026-10-26 Created.

////////

use app_core::models::user::UserProfile;
use app_core::ports::identity::UserIdentityRepository;
use app_core::ports::message::ContactRelationshipRepository;
use async_trait::async_trait;
use tracing::{info, warn};

////////

/// # [POLICY] - 单聊准入策略
/// * `DESC`: `决定本地缺少联系人关系时的处理方式`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectMessagePolicy {
    /// 严格：只信任本地 `contact_list`（生产默认，关系由主后端同步写入）
    Strict,
    /// 资料校验：本地缺失时向主后端取资料，命中后注入联系人模型并放行（联调 / 灰度）
    Profile,
    /// 不校验：仅用于压测与本地调试
    Allow,
}

////////

impl DirectMessagePolicy {
    /// # [HANDLER] - parse
    /// * `DESC`: `解析配置字符串 strict / profile / allow`
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "strict" => Ok(Self::Strict),
            "profile" => Ok(Self::Profile),
            "allow" => Ok(Self::Allow),
            other => Err(format!(
                "im.direct.policy 非法: '{other}'（可选 strict / profile / allow）"
            )),
        }
    }

    /// # [HANDLER] - as_str
    /// * `DESC`: `策略名称，用于启动日志`
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Profile => "profile",
            Self::Allow => "allow",
        }
    }
}

////////

/// # [SERVICE] - 联系人模型同步网关
/// * `DESC`: `包装本地联系人仓储；本地判定失败且策略为 profile 时，
///   通过 UserIdentityRepository（HTTP / gRPC）拉取远端资料并注入 contact_profile + contact_list`
#[derive(Clone)]
pub struct SyncingContactGateway<C, I> {
    local: C,
    identity: I,
    policy: DirectMessagePolicy,
}

////////

impl<C, I> SyncingContactGateway<C, I>
where
    C: ContactRelationshipRepository,
    I: UserIdentityRepository,
{
    /// # [SERVICE] - new
    /// * `DESC`: `注入本地仓储、远端资料客户端与准入策略`
    pub fn new(local: C, identity: I, policy: DirectMessagePolicy) -> Self {
        info!(
            target: "im::contact_sync",
            policy = policy.as_str(),
            transport = identity.transport(),
            "[🧭 CONTACT SYNC] 联系人同步网关初始化完成"
        );
        Self {
            local,
            identity,
            policy,
        }
    }

    /// # [SERVICE] - inject_remote_contact
    /// * `DESC`: `拉取远端资料并注入本地联系人模型；远端账号不存在或已封禁时拒绝`
    async fn inject_remote_contact(
        &self,
        owner_account_id: u64,
        target_account_id: u64,
    ) -> Result<bool, String> {
        let Some(profile) = self.identity.fetch_profile(target_account_id).await? else {
            info!(
                target: "im::contact_sync",
                owner_account_id = owner_account_id,
                target_account_id = target_account_id,
                transport = self.identity.transport(),
                "[🧭 CONTACT SYNC] 远端账号不存在，拒绝单聊"
            );
            return Ok(false);
        };

        if !profile.is_active() {
            warn!(
                target: "im::contact_sync",
                target_account_id = target_account_id,
                status = profile.status.unwrap_or_default(),
                "[🧭 CONTACT SYNC] 远端账号状态不可用，拒绝单聊"
            );
            return Ok(false);
        }

        self.local.upsert_contact_profile(&profile).await?;
        self.local
            .ensure_contact_relation(owner_account_id, target_account_id)
            .await?;

        info!(
            target: "im::contact_sync",
            owner_account_id = owner_account_id,
            target_account_id = target_account_id,
            nickname = %profile.nickname,
            transport = self.identity.transport(),
            "[🧭 CONTACT SYNC] 远端资料已注入本地联系人模型"
        );
        Ok(true)
    }
}

////////

#[async_trait]
impl<C, I> ContactRelationshipRepository for SyncingContactGateway<C, I>
where
    C: ContactRelationshipRepository,
    I: UserIdentityRepository,
{
    /// 行为 1：本地优先判定；缺失时按策略回落远端资料并补建关系
    async fn can_message(
        &self,
        sender_account_id: u64,
        recipient_account_id: u64,
    ) -> Result<bool, String> {
        match self.policy {
            DirectMessagePolicy::Allow => Ok(true),
            DirectMessagePolicy::Strict => {
                self.local
                    .can_message(sender_account_id, recipient_account_id)
                    .await
            }
            DirectMessagePolicy::Profile => {
                if self
                    .local
                    .can_message(sender_account_id, recipient_account_id)
                    .await?
                {
                    return Ok(true);
                }

                self.inject_remote_contact(sender_account_id, recipient_account_id)
                    .await
            }
        }
    }

    /// 行为 2：资料写入始终落到本地仓储
    async fn upsert_contact_profile(&self, profile: &UserProfile) -> Result<(), String> {
        self.local.upsert_contact_profile(profile).await
    }

    /// 行为 3：关系补建始终落到本地仓储
    async fn ensure_contact_relation(
        &self,
        owner_account_id: u64,
        target_account_id: u64,
    ) -> Result<(), String> {
        self.local
            .ensure_contact_relation(owner_account_id, target_account_id)
            .await
    }
}

////////

#[cfg(test)]
mod tests {
    use super::{DirectMessagePolicy, SyncingContactGateway};
    use app_core::models::user::UserProfile;
    use app_core::ports::identity::UserIdentityRepository;
    use app_core::ports::message::ContactRelationshipRepository;
    use async_trait::async_trait;
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct FakeContacts {
        relations: Arc<Mutex<HashSet<(u64, u64)>>>,
        profiles: Arc<Mutex<Vec<u64>>>,
    }

    impl FakeContacts {
        fn with_relation(owner: u64, target: u64) -> Self {
            let this = Self::default();
            this.relations.lock().unwrap().insert((owner, target));
            this
        }

        fn has_relation(&self, owner: u64, target: u64) -> bool {
            self.relations.lock().unwrap().contains(&(owner, target))
        }

        fn stored_profiles(&self) -> Vec<u64> {
            self.profiles.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl ContactRelationshipRepository for FakeContacts {
        async fn can_message(&self, owner: u64, target: u64) -> Result<bool, String> {
            Ok(self.has_relation(owner, target))
        }

        async fn upsert_contact_profile(&self, profile: &UserProfile) -> Result<(), String> {
            self.profiles.lock().unwrap().push(profile.account_id);
            Ok(())
        }

        async fn ensure_contact_relation(&self, owner: u64, target: u64) -> Result<(), String> {
            self.relations.lock().unwrap().insert((owner, target));
            Ok(())
        }
    }

    #[derive(Clone)]
    struct FakeIdentity {
        profile: Option<UserProfile>,
        fail: bool,
        calls: Arc<AtomicUsize>,
    }

    impl FakeIdentity {
        fn found(profile: UserProfile) -> Self {
            Self {
                profile: Some(profile),
                fail: false,
                calls: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn missing() -> Self {
            Self {
                profile: None,
                fail: false,
                calls: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn failing() -> Self {
            Self {
                profile: None,
                fail: true,
                calls: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl UserIdentityRepository for FakeIdentity {
        async fn fetch_profile(&self, _: u64) -> Result<Option<UserProfile>, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err("remote profile service is down".to_owned());
            }
            Ok(self.profile.clone())
        }

        fn transport(&self) -> &'static str {
            "fake"
        }
    }

    fn sample_profile(account_id: u64) -> UserProfile {
        UserProfile {
            account_id,
            nickname: "可乐的朋友".to_owned(),
            avatar_url: Some("avatar.jpg".to_owned()),
            signature: Some("hi".to_owned()),
            gender: Some(1),
            status: Some(1),
        }
    }

    const SENDER: u64 = 1000020104;
    const RECIPIENT: u64 = 1000020102;

    /// # [HANDLER] - profile_policy_injects_remote_contact
    /// * `DESC`: `本地无关系 + 远端有资料 → 放行，并写入资料与联系人关系`
    #[tokio::test]
    async fn profile_policy_injects_remote_contact() {
        let local = FakeContacts::default();
        let identity = FakeIdentity::found(sample_profile(RECIPIENT));
        let gateway = SyncingContactGateway::new(
            local.clone(),
            identity.clone(),
            DirectMessagePolicy::Profile,
        );

        assert!(gateway
            .can_message(SENDER, RECIPIENT)
            .await
            .expect("can_message should not fail"));
        assert_eq!(identity.calls(), 1);
        assert_eq!(local.stored_profiles(), vec![RECIPIENT]);
        assert!(local.has_relation(SENDER, RECIPIENT));
        assert!(
            !local.has_relation(RECIPIENT, SENDER),
            "不应替对方伪造通讯录条目"
        );
    }

    /// # [HANDLER] - profile_policy_rejects_unknown_account
    /// * `DESC`: `远端无资料 → 拒绝，且不写入任何联系人数据`
    #[tokio::test]
    async fn profile_policy_rejects_unknown_account() {
        let local = FakeContacts::default();
        let gateway = SyncingContactGateway::new(
            local.clone(),
            FakeIdentity::missing(),
            DirectMessagePolicy::Profile,
        );

        assert!(!gateway
            .can_message(SENDER, RECIPIENT)
            .await
            .expect("can_message should not fail"));
        assert!(local.stored_profiles().is_empty());
        assert!(!local.has_relation(SENDER, RECIPIENT));
    }

    /// # [HANDLER] - profile_policy_rejects_inactive_account
    /// * `DESC`: `远端账号被封禁 → 拒绝`
    #[tokio::test]
    async fn profile_policy_rejects_inactive_account() {
        let local = FakeContacts::default();
        let mut profile = sample_profile(RECIPIENT);
        profile.status = Some(3);
        let gateway = SyncingContactGateway::new(
            local.clone(),
            FakeIdentity::found(profile),
            DirectMessagePolicy::Profile,
        );

        assert!(!gateway
            .can_message(SENDER, RECIPIENT)
            .await
            .expect("can_message should not fail"));
        assert!(local.stored_profiles().is_empty());
    }

    /// # [HANDLER] - profile_policy_short_circuits_on_local_hit
    /// * `DESC`: `本地已有关系时不再调用远端`
    #[tokio::test]
    async fn profile_policy_short_circuits_on_local_hit() {
        let identity = FakeIdentity::found(sample_profile(RECIPIENT));
        let gateway = SyncingContactGateway::new(
            FakeContacts::with_relation(SENDER, RECIPIENT),
            identity.clone(),
            DirectMessagePolicy::Profile,
        );

        assert!(gateway
            .can_message(SENDER, RECIPIENT)
            .await
            .expect("can_message should not fail"));
        assert_eq!(identity.calls(), 0);
    }

    /// # [HANDLER] - strict_policy_never_calls_remote
    /// * `DESC`: `strict 策略只信任本地关系，且不发起跨进程调用`
    #[tokio::test]
    async fn strict_policy_never_calls_remote() {
        let identity = FakeIdentity::found(sample_profile(RECIPIENT));
        let gateway = SyncingContactGateway::new(
            FakeContacts::default(),
            identity.clone(),
            DirectMessagePolicy::Strict,
        );

        assert!(!gateway
            .can_message(SENDER, RECIPIENT)
            .await
            .expect("can_message should not fail"));
        assert_eq!(identity.calls(), 0);
    }

    /// # [HANDLER] - allow_policy_bypasses_contacts
    /// * `DESC`: `allow 策略直接放行且不发起跨进程调用`
    #[tokio::test]
    async fn allow_policy_bypasses_contacts() {
        let identity = FakeIdentity::missing();
        let gateway = SyncingContactGateway::new(
            FakeContacts::default(),
            identity.clone(),
            DirectMessagePolicy::Allow,
        );

        assert!(gateway
            .can_message(SENDER, RECIPIENT)
            .await
            .expect("can_message should not fail"));
        assert_eq!(identity.calls(), 0);
    }

    /// # [HANDLER] - remote_failure_propagates
    /// * `DESC`: `远端故障时向上抛出错误，由调用方拒绝本次发送而不是静默放行`
    #[tokio::test]
    async fn remote_failure_propagates() {
        let gateway = SyncingContactGateway::new(
            FakeContacts::default(),
            FakeIdentity::failing(),
            DirectMessagePolicy::Profile,
        );

        assert!(gateway.can_message(SENDER, RECIPIENT).await.is_err());
    }

    /// # [HANDLER] - live_gateway_and_pg_roundtrip
    /// * `DESC`: `真实链路自检：HTTP 拉取主后端资料 → 注入 contact_profile/contact_list → can_message 放行 → 清理探测关系`
    /// * `NOTE`: `需要 127.0.0.1:8080 网关与 PostgreSQL im_2024 可用：
    ///   cargo test -p cola-im-chat-server --bins live_gateway -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "requires the main backend gateway on 127.0.0.1:8080 and a live PostgreSQL im_2024"]
    async fn live_gateway_and_pg_roundtrip() {
        use crate::config::IdentitySettings;
        use crate::service::identity::HttpUserIdentity;
        use storage_pg::repository::contact::PgContactRelationshipRepository;

        let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            // 与服务端一致：lc_messages=C 避免 GBK locale 让 sqlx 读到非 UTF-8 错误消息
            "postgres://postgres:123456@127.0.0.1:5432/im_2024?client_encoding=UTF8&lc_messages=C"
                .to_owned()
        });
        let pool = sqlx::PgPool::connect(&url)
            .await
            .expect("PostgreSQL 应可连接");
        let identity = HttpUserIdentity::new(&IdentitySettings::default());
        let gateway = SyncingContactGateway::new(
            PgContactRelationshipRepository::new(pool.clone()),
            identity,
            DirectMessagePolicy::Profile,
        );

        // 合成发送者 + 主后端真实账号「可乐」(1000008801)
        let sender: i64 = 9_990_000_001;
        let recipient: i64 = 1_000_008_801;

        // 1. 保证起点没有本地关系，强制走远端资料回落分支
        sqlx::query("DELETE FROM im_2024.contact_list WHERE user_id = $1 AND target_id = $2")
            .bind(sender)
            .bind(recipient)
            .execute(&pool)
            .await
            .expect("清理探测关系应成功");

        // 2. 真实跨进程调用 + 落库
        let allowed = gateway
            .can_message(sender as u64, recipient as u64)
            .await
            .expect("can_message 不应报错");
        assert!(allowed, "远端资料命中后应放行单聊");

        // 3. 资料已写入 contact_profile（以主后端资料为准，可重复覆盖）
        let name: Option<String> =
            sqlx::query_scalar("SELECT name FROM im_2024.contact_profile WHERE id = $1")
                .bind(recipient)
                .fetch_optional(&pool)
                .await
                .expect("查询 contact_profile 应成功");
        assert_eq!(name.as_deref(), Some("可乐"));

        // 4. 关系已补建且对 can_message 可见
        let relation: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM im_2024.contact_list \
             WHERE user_id = $1 AND target_id = $2 AND status = 1 AND is_deleted = FALSE)",
        )
        .bind(sender)
        .bind(recipient)
        .fetch_one(&pool)
        .await
        .expect("查询 contact_list 应成功");
        assert!(relation, "contact_list 应补建成功");

        // 5. 清理探测关系，不污染真实通讯录
        sqlx::query("DELETE FROM im_2024.contact_list WHERE user_id = $1 AND target_id = $2")
            .bind(sender)
            .bind(recipient)
            .execute(&pool)
            .await
            .expect("清理探测关系应成功");
    }

    /// # [HANDLER] - policy_parsing_covers_all_branches
    /// * `DESC`: `验证策略解析与非法值拒绝`
    #[test]
    fn policy_parsing_covers_all_branches() {
        assert_eq!(
            DirectMessagePolicy::parse(" Strict ").unwrap(),
            DirectMessagePolicy::Strict
        );
        assert_eq!(
            DirectMessagePolicy::parse("profile").unwrap(),
            DirectMessagePolicy::Profile
        );
        assert_eq!(
            DirectMessagePolicy::parse("ALLOW").unwrap(),
            DirectMessagePolicy::Allow
        );
        assert!(DirectMessagePolicy::parse("open").is_err());
    }
}

//////// END
