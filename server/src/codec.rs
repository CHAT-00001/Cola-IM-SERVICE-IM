// server/src/codec.rs -- Transport-independent message codec interfaces
// 2026/10/7 14:03 Created.

////////

use crate::models::message::MessageBody;

////////

/// # [HANDLER] - MessageCodec
/// * `DESC`: `传输无关的消息编解码器接口`
pub trait MessageCodec: Send + Sync {
    fn decode(&self, frame: &str) -> Result<MessageBody, serde_json::Error>;

    fn encode(&self, message: &MessageBody) -> Result<String, serde_json::Error>;
}

////////

/// # [HANDLER] - JsonCodec
/// * `DESC`: `JSON 编解码器实现`
pub struct JsonCodec;

impl MessageCodec for JsonCodec {
    fn decode(&self, frame: &str) -> Result<MessageBody, serde_json::Error> {
        serde_json::from_str(frame)
    }

    fn encode(&self, message: &MessageBody) -> Result<String, serde_json::Error> {
        serde_json::to_string(message)
    }
}

////////

#[cfg(test)]
mod tests {
    use super::{JsonCodec, MessageCodec};

    /// # [HANDLER] - json_codec_rejects_invalid_message
    /// * `DESC`: `验证 JSON 编解码器拒绝非法消息`
    #[test]
    fn json_codec_rejects_invalid_message() {
        assert!(JsonCodec.decode("not-json").is_err());
    }
}

//////// END
