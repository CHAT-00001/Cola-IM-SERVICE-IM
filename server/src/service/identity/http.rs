// server/src/service/identity/http.rs -- 用户资料 - HTTP 传输实现
// 2026-10-26 Created.

////////

use std::time::Duration;

use app_core::models::user::UserProfile;
use app_core::ports::identity::UserIdentityRepository;
use async_trait::async_trait;
use serde::Deserialize;
use tracing::{info, warn};

use crate::config::IdentitySettings;

////////

/// # [ADAPTER] - HTTP 用户资料客户端
/// * `DESC`: `通过主后端网关（service=get_profile）拉取用户资料；无状态、复用连接池，
///   默认指向 http://127.0.0.1:8080/api/v2/user/gateway?service=get_profile&id={account_id}`
#[derive(Clone)]
pub struct HttpUserIdentity {
    client: reqwest::Client,
    base_url: String,
    path: String,
    service: String,
    timeout: Duration,
    access_token: Option<String>,
}

////////

/// # [DTO] - 主后端统一响应信封
#[derive(Debug, Deserialize)]
struct ProfileEnvelope {
    #[serde(default)]
    code: i32,
    #[serde(default)]
    message: String,
    #[serde(default)]
    data: Option<ProfilePayload>,
}

////////

/// # [DTO] - 资料名片负载（只取 IM 需要的字段，其余字段忽略）
#[derive(Debug, Deserialize)]
struct ProfilePayload {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    nickname: Option<String>,
    #[serde(default)]
    avatar_url: Option<String>,
    #[serde(default)]
    signature: Option<String>,
    #[serde(default)]
    gender: Option<i16>,
    #[serde(default)]
    status: Option<i16>,
}

////////

impl HttpUserIdentity {
    /// # [ADAPTER] - new
    /// * `DESC`: `按配置构建 HTTP 资料客户端（超时与网关地址来自 im.identity）`
    pub fn new(settings: &IdentitySettings) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: settings.base_url.trim_end_matches('/').to_owned(),
            path: settings.path.clone(),
            service: settings.service.clone(),
            timeout: Duration::from_millis(settings.timeout_ms),
            access_token: settings
                .token
                .as_deref()
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .map(str::to_owned),
        }
    }

    /// # [ADAPTER] - profile_url
    /// * `DESC`: `拼接 get_profile 请求地址（与主后端网关 v2 约定一致）`
    pub fn profile_url(&self, account_id: u64) -> String {
        let mut url = format!(
            "{}{}?service={}&id={}",
            self.base_url, self.path, self.service, account_id
        );
        if let Some(token) = self.access_token.as_deref() {
            url.push_str("&access_token=");
            url.push_str(token);
        }
        url
    }

    /// # [ADAPTER] - parse_profile
    /// * `DESC`: `解析主后端响应信封；code!=0 视为失败，data=null 视为账号不存在`
    pub fn parse_profile(
        body: &str,
        requested_account_id: u64,
    ) -> Result<Option<UserProfile>, String> {
        let envelope: ProfileEnvelope = serde_json::from_str(body)
            .map_err(|error| format!("用户资料响应不是合法 JSON: {error}; body={body}"))?;

        if envelope.code != 0 {
            return Err(format!(
                "用户资料接口返回业务错误: code={} message={}",
                envelope.code, envelope.message
            ));
        }

        let Some(payload) = envelope.data else {
            return Ok(None);
        };

        let account_id = u64::try_from(payload.id)
            .ok()
            .filter(|id| *id > 0)
            .unwrap_or(requested_account_id);

        Ok(Some(UserProfile {
            account_id,
            nickname: payload.nickname.unwrap_or_default(),
            avatar_url: payload.avatar_url.filter(|value| !value.trim().is_empty()),
            signature: payload.signature.filter(|value| !value.trim().is_empty()),
            gender: payload.gender,
            status: payload.status,
        }))
    }
}

////////

#[async_trait]
impl UserIdentityRepository for HttpUserIdentity {
    async fn fetch_profile(&self, account_id: u64) -> Result<Option<UserProfile>, String> {
        let url = self.profile_url(account_id);
        let response = self
            .client
            .get(&url)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|error| format!("请求用户资料接口失败: {error}; url={url}"))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| format!("读取用户资料响应失败: {error}; url={url}"))?;

        if !status.is_success() {
            warn!(
                target: "im::identity::http",
                account_id = account_id,
                status = status.as_u16(),
                "[📡 IDENTITY HTTP] 用户资料接口返回非 2xx"
            );
            return Err(format!(
                "用户资料接口返回 HTTP {}; url={url}; body={body}",
                status.as_u16()
            ));
        }

        let profile = Self::parse_profile(&body, account_id)?;
        info!(
            target: "im::identity::http",
            account_id = account_id,
            found = profile.is_some(),
            "[📡 IDENTITY HTTP] 用户资料拉取完成"
        );
        Ok(profile)
    }

    fn transport(&self) -> &'static str {
        "http"
    }
}

////////

#[cfg(test)]
mod tests {
    use super::HttpUserIdentity;
    use crate::config::IdentitySettings;

    /// 主后端 `service=get_profile` 的真实响应样例（2026-10-26 抓取）。
    const SAMPLE_BODY: &str = r#"{"code":0,"message":"资料名片查询成功","duration":"2.205ms","request_id":"73c4bc2e-2471-464b-8c3f-21fb76a030b2","at":"2026-10-25T19:52:56.960Z","log_id":"2024","data":{"id":1000008801,"nickname":"可乐","avatar_url":"android_10008801_20230911_012215_3209388.jpg","bg_img":"qiniu_android_10008801_20230818_221056_0527065.jpg","signature":"我那么可爱的","birthday":992793600,"add_time":1592410082,"status":1,"age":25,"is_following":false,"is_online":false,"is_streaming":false}}"#;

    /// # [HANDLER] - http_probe_url_matches_gateway_contract
    /// * `DESC`: `验证拼出的地址与主后端网关约定一致`
    #[test]
    fn http_probe_url_matches_gateway_contract() {
        let settings = IdentitySettings::default();
        let client = HttpUserIdentity::new(&settings);

        assert_eq!(
            client.profile_url(1000008801),
            "http://127.0.0.1:8080/api/v2/user/gateway?service=get_profile&id=1000008801"
        );
    }

    /// # [HANDLER] - parses_real_profile_payload
    /// * `DESC`: `验证真实响应可解析为资料快照`
    #[test]
    fn parses_real_profile_payload() {
        let profile = HttpUserIdentity::parse_profile(SAMPLE_BODY, 1000008801)
            .expect("sample payload should parse")
            .expect("sample payload carries data");

        assert_eq!(profile.account_id, 1000008801);
        assert_eq!(profile.nickname, "可乐");
        assert_eq!(
            profile.avatar_url.as_deref(),
            Some("android_10008801_20230911_012215_3209388.jpg")
        );
        assert_eq!(profile.signature.as_deref(), Some("我那么可爱的"));
        assert_eq!(profile.status, Some(1));
        assert!(profile.is_active());
    }

    /// # [HANDLER] - null_data_means_account_missing
    /// * `DESC`: `验证 data=null 表示账号不存在而不是错误`
    #[test]
    fn null_data_means_account_missing() {
        let body = r#"{"code":0,"message":"资料名片不存在","data":null}"#;
        let profile = HttpUserIdentity::parse_profile(body, 1000020102).expect("envelope parses");
        assert!(profile.is_none());
    }

    /// # [HANDLER] - business_error_is_reported
    /// * `DESC`: `验证业务错误码不会被误判为账号不存在`
    #[test]
    fn business_error_is_reported() {
        let body = r#"{"code":4003,"message":"权限不足","data":null}"#;
        let error = HttpUserIdentity::parse_profile(body, 1000020102)
            .expect_err("business error must surface as Err");
        assert!(error.contains("4003"), "unexpected error: {error}");
    }

    /// # [HANDLER] - configured_token_is_appended
    /// * `DESC`: `验证配置了内部令牌时会附加 access_token 参数`
    #[test]
    fn configured_token_is_appended() {
        let settings = IdentitySettings {
            token: Some("internal-token".to_owned()),
            ..IdentitySettings::default()
        };
        let client = HttpUserIdentity::new(&settings);

        assert!(client
            .profile_url(1000008801)
            .ends_with("&access_token=internal-token"));
    }
}

//////// END
