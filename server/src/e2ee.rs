// server/src/e2ee.rs -- End-to-end encryption hook (transport-independent)
// 2026/10/27 Created.

////////

use crate::chat::model::AuthenticatedSession;

////////

/// # [HANDLER] - E2eeError
/// * `DESC`: `端到端加密钩子的错误类型，解密失败时消息必须被丢弃`
#[derive(Debug, thiserror::Error)]
pub enum E2eeError {
    #[error("e2ee decrypt failed: {0}")]
    Decrypt(String),
    #[error("e2ee encrypt failed: {0}")]
    Encrypt(String),
}

////////

/// # [HANDLER] - E2eeHook
/// * `DESC`: `端到端加密对接点。服务端只转发密文，编解码前后分别调用 open/seal。
///   服务端默认不持有密钥，实现方应只处理 payload 字节，不改变路由字段。`
pub trait E2eeHook: Send + Sync {
    /// 解密客户端上行的编码帧（codec 解码之前调用）
    fn open(&self, session: &AuthenticatedSession, frame: &[u8]) -> Result<Vec<u8>, E2eeError>;

    /// 加密下行编码帧（codec 编码之后调用）
    fn seal(&self, session: &AuthenticatedSession, frame: &[u8]) -> Result<Vec<u8>, E2eeError>;
}

////////

/// # [HANDLER] - PassthroughE2ee
/// * `DESC`: `默认实现：不加密，直接透传字节。E2EE 启用前的占位实现`
pub struct PassthroughE2ee;

impl E2eeHook for PassthroughE2ee {
    fn open(&self, _session: &AuthenticatedSession, frame: &[u8]) -> Result<Vec<u8>, E2eeError> {
        Ok(frame.to_vec())
    }

    fn seal(&self, _session: &AuthenticatedSession, frame: &[u8]) -> Result<Vec<u8>, E2eeError> {
        Ok(frame.to_vec())
    }
}

//////// END
