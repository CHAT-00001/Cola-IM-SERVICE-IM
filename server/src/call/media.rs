// server/src/call/media.rs -- 媒体面抽象（当前端到端 WebRTC，中央推流预留）
// 2026-11-12 16:45 Created.
//
// 现在：媒体走端到端 WebRTC，服务端只转发 SDP/ICE，不承载音视频流量（省钱）。
// 以后：新增独立 stream-server 或接第三方（LiveKit 等）时，实现 MediaPlane 即可，
//       信令协议与通话记录不变。

////////

use async_trait::async_trait;

use super::model::CallKind;

////////

/// # [MODEL] - 媒体模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaMode {
    /// 端到端 P2P（当前）
    P2p,
    /// 中央推流（预留：SFU / stream-server / 第三方）
    Sfu,
}

impl MediaMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::P2p => "p2p",
            Self::Sfu => "sfu",
        }
    }
}

////////

/// # [MODEL] - 中央推流凭证（预留）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaTicket {
    pub server_url: String,
    pub room: String,
    pub token: String,
}

////////

/// # [PORT] - 媒体面
#[async_trait]
pub trait MediaPlane: Send + Sync {
    fn mode(&self) -> MediaMode;

    /// 为一次通话分配媒体资源；P2P 模式返回 None
    async fn provision(&self, call_id: &str, kind: CallKind) -> Result<Option<MediaTicket>, String>;

    /// 通话结束时释放资源
    async fn release(&self, call_id: &str);
}

////////

/// # [PLANE] - P2P 媒体面
/// * `DESC`: `端到端 WebRTC，服务端不分配任何媒体资源`
#[derive(Debug, Default)]
pub struct P2pMediaPlane;

#[async_trait]
impl MediaPlane for P2pMediaPlane {
    fn mode(&self) -> MediaMode {
        MediaMode::P2p
    }

    async fn provision(&self, _call_id: &str, _kind: CallKind) -> Result<Option<MediaTicket>, String> {
        Ok(None)
    }

    async fn release(&self, _call_id: &str) {}
}

////////

/// # [PLANE] - 中央推流媒体面（预留）
/// * `DESC`: `接入独立 stream-server 或第三方 RTC 时实现；当前返回明确的未接入错误`
#[derive(Debug, Clone)]
pub struct CentralMediaPlane {
    pub endpoint: String,
}

#[async_trait]
impl MediaPlane for CentralMediaPlane {
    fn mode(&self) -> MediaMode {
        MediaMode::Sfu
    }

    async fn provision(&self, call_id: &str, _kind: CallKind) -> Result<Option<MediaTicket>, String> {
        Err(format!(
            "central media plane 未接入: endpoint={}, call_id={}",
            self.endpoint, call_id
        ))
    }

    async fn release(&self, _call_id: &str) {}
}

////////

#[cfg(test)]
mod tests {
    use super::{CentralMediaPlane, MediaMode, MediaPlane, P2pMediaPlane};
    use crate::call::model::CallKind;

    #[tokio::test]
    async fn p2p_plane_allocates_nothing() {
        let plane = P2pMediaPlane;
        assert_eq!(plane.mode(), MediaMode::P2p);
        assert_eq!(plane.provision("c1", CallKind::Voice).await.unwrap(), None);
    }

    #[tokio::test]
    async fn central_plane_is_reserved_but_explicit() {
        let plane = CentralMediaPlane {
            endpoint: "wss://stream.example.com".to_owned(),
        };
        assert_eq!(plane.mode(), MediaMode::Sfu);
        let error = plane.provision("c1", CallKind::Video).await.unwrap_err();
        assert!(error.contains("未接入"));
    }
}

//////// END
