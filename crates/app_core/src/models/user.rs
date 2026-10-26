use serde::{Deserialize, Serialize};

/// 用户信息
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserInfo {
    pub id: u64,
    pub user_nickname: String,
    pub avatar: String,
    pub level: String,
    pub usertype: String,
    pub vip_type: String,
    pub guard_type: String,
    pub liangname: String,
    pub car_id: String,
    pub car_swf: String,
    pub car_swftime: String,
    pub car_words: String,
}

////////

/// # [MODEL] - 用户资料快照
/// * `DESC`: `主后端用户服务（get_profile）返回的资料副本，IM 侧只做缓存与联系人档案注入`
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct UserProfile {
    /// 账号 ID（主后端 uid）
    pub account_id: u64,
    /// 昵称
    pub nickname: String,
    /// 头像地址（主后端返回的可能是不带 CDN 前缀的文件名）
    pub avatar_url: Option<String>,
    /// 个性签名
    pub signature: Option<String>,
    /// 性别（沿用主后端编码，IM 侧不解释）
    pub gender: Option<i16>,
    /// 账号状态: 0.失效 1.正常 2.异常 3.封禁
    pub status: Option<i16>,
}

////////

impl UserProfile {
    /// # [HANDLER] - is_active
    /// * `DESC`: `账号是否可用于单聊（状态缺失按正常处理）`
    pub fn is_active(&self) -> bool {
        matches!(self.status, None | Some(1))
    }
}
