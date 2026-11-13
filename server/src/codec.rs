// server/src/codec.rs -- Transport-independent message codec interfaces
// 2026/10/7 14:03 Created.

////////

use chrono::{DateTime, Utc};

use crate::models::message::{AckStage, MessageBody, Payload};

////////

/// # [HANDLER] - CodecError
/// * `DESC`: `传输无关的编解码错误，统一 JSON 与 protobuf 两种格式的失败原因`
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("json codec error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("protobuf decode error: {0}")]
    ProtoDecode(#[from] prost::DecodeError),
    #[error("protobuf field error: {0}")]
    ProtoField(String),
}

////////

/// # [HANDLER] - WireFormat
/// * `DESC`: `线上数据格式枚举，由 WT 连接的 format 参数或 WS 默认值决定`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireFormat {
    Json,
    Protobuf,
}

impl WireFormat {
    /// # [HANDLER] - parse
    /// * `DESC`: `解析协商参数，未知值返回 None`
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "json" => Some(Self::Json),
            "protobuf" | "proto" | "pb" => Some(Self::Protobuf),
            _ => None,
        }
    }

    /// # [HANDLER] - codec
    /// * `DESC`: `按格式返回对应的编解码器实例`
    pub fn codec(self) -> Box<dyn MessageCodec> {
        match self {
            Self::Json => Box::new(JsonCodec),
            Self::Protobuf => Box::new(ProtobufCodec),
        }
    }
}

////////

/// # [HANDLER] - MessageCodec
/// * `DESC`: `传输无关的字节级消息编解码器接口，WS 文本帧与 WT 长度前缀帧共用`
pub trait MessageCodec: Send + Sync {
    /// 线上格式标识，用于协商与日志
    fn format(&self) -> WireFormat;

    fn decode(&self, frame: &[u8]) -> Result<MessageBody, CodecError>;

    fn encode(&self, message: &MessageBody) -> Result<Vec<u8>, CodecError>;
}

////////

/// # [HANDLER] - JsonCodec
/// * `DESC`: `JSON 编解码器实现（UTF-8 文本）`
pub struct JsonCodec;

impl MessageCodec for JsonCodec {
    fn format(&self) -> WireFormat {
        WireFormat::Json
    }

    fn decode(&self, frame: &[u8]) -> Result<MessageBody, CodecError> {
        Ok(serde_json::from_slice(frame)?)
    }

    fn encode(&self, message: &MessageBody) -> Result<Vec<u8>, CodecError> {
        Ok(serde_json::to_vec(message)?)
    }
}

////////

/// # [HANDLER] - ProtobufCodec
/// * `DESC`: `protobuf 编解码器实现，负载结构见 ProtoMessage`
pub struct ProtobufCodec;

impl MessageCodec for ProtobufCodec {
    fn format(&self) -> WireFormat {
        WireFormat::Protobuf
    }

    fn decode(&self, frame: &[u8]) -> Result<MessageBody, CodecError> {
        let wire = <ProtoMessage as prost::Message>::decode(frame)?;
        wire.into_body()
    }

    fn encode(&self, message: &MessageBody) -> Result<Vec<u8>, CodecError> {
        let wire = ProtoMessage::from_body(message)?;
        Ok(prost::Message::encode_to_vec(&wire))
    }
}

////////

/// # [HANDLER] - ProtoMessage
/// * `DESC`: `protobuf 线上消息结构；标量字段直接映射，复杂负载以 JSON 字节承载`
#[derive(Clone, PartialEq, prost::Message)]
pub struct ProtoMessage {
    #[prost(uint32, tag = "1")]
    pub status: u32,
    #[prost(string, tag = "2")]
    pub client_id: String,
    #[prost(int64, tag = "3")]
    pub send_at_ms: i64,
    #[prost(int64, optional, tag = "4")]
    pub server_sync_at_ms: Option<i64>,
    #[prost(string, tag = "5")]
    pub send_id: String,
    #[prost(string, tag = "6")]
    pub sync_id: String,
    #[prost(uint32, tag = "7")]
    pub msg_type: u32,
    #[prost(uint32, tag = "8")]
    pub event: u32,
    #[prost(string, tag = "9")]
    pub room_id: String,
    #[prost(uint64, tag = "10")]
    pub uid: u64,
    #[prost(bytes = "vec", optional, tag = "11")]
    pub sender_json: Option<Vec<u8>>,
    #[prost(uint32, tag = "12")]
    pub payload_kind: u32,
    #[prost(string, tag = "13")]
    pub payload_text: String,
    #[prost(bytes = "vec", tag = "14")]
    pub payload_json: Vec<u8>,
    #[prost(uint32, optional, tag = "15")]
    pub channel: Option<u32>,
    #[prost(uint64, optional, tag = "16")]
    pub recipient_uid: Option<u64>,
    #[prost(string, optional, tag = "17")]
    pub ack_message_id: Option<String>,
    #[prost(string, optional, tag = "18")]
    pub device_id: Option<String>,
    #[prost(string, optional, tag = "19")]
    pub ack_stage: Option<String>,
}

const PAYLOAD_SYSTEM: u32 = 0;
const PAYLOAD_CONTENT: u32 = 1;
const PAYLOAD_JSON: u32 = 2;

/// # [HANDLER] - repr_code
/// * `DESC`: `把数字编码的枚举（serde_repr）转换为线上数字码`
fn repr_code<T: serde::Serialize>(value: &T) -> Result<u32, CodecError> {
    serde_json::to_value(value)?
        .as_u64()
        .and_then(|code| u32::try_from(code).ok())
        .ok_or_else(|| CodecError::ProtoField("expected numeric enum".to_owned()))
}

/// # [HANDLER] - repr_from_code
/// * `DESC`: `从线上数字码还原数字编码的枚举，未知码返回错误`
fn repr_from_code<T: serde::de::DeserializeOwned>(code: u32) -> Result<T, CodecError> {
    serde_json::from_value(serde_json::Value::from(code))
        .map_err(|_| CodecError::ProtoField(format!("unknown enum code {code}")))
}

/// # [HANDLER] - timestamp
/// * `DESC`: `毫秒时间戳转换为 UTC 时间，越界返回错误`
fn timestamp(millis: i64) -> Result<DateTime<Utc>, CodecError> {
    DateTime::<Utc>::from_timestamp_millis(millis)
        .ok_or_else(|| CodecError::ProtoField(format!("invalid timestamp {millis}")))
}

/// # [HANDLER] - ack_stage_name / ack_stage_from_name
/// * `DESC`: `确认阶段使用 snake_case 字符串枚举，与 JSON 格式保持一致`
fn ack_stage_name(stage: &AckStage) -> Result<String, CodecError> {
    match serde_json::to_value(stage)? {
        serde_json::Value::String(name) => Ok(name),
        _ => Err(CodecError::ProtoField(
            "expected string ack stage".to_owned(),
        )),
    }
}

fn ack_stage_from_name(name: &str) -> Result<AckStage, CodecError> {
    serde_json::from_value(serde_json::Value::String(name.to_owned()))
        .map_err(|_| CodecError::ProtoField(format!("unknown ack stage {name}")))
}

impl ProtoMessage {
    /// # [HANDLER] - from_body
    /// * `DESC`: `将领域消息体编码为 protobuf 线上结构`
    fn from_body(body: &MessageBody) -> Result<Self, CodecError> {
        let (payload_kind, payload_text, payload_json) = match &body.payload {
            Payload::System { message } => (PAYLOAD_SYSTEM, message.clone(), Vec::new()),
            Payload::Content { text } => (PAYLOAD_CONTENT, text.clone(), Vec::new()),
            other => (PAYLOAD_JSON, String::new(), serde_json::to_vec(other)?),
        };
        Ok(Self {
            status: repr_code(&body.status)?,
            client_id: body.client_id.clone(),
            send_at_ms: body.send_at.timestamp_millis(),
            server_sync_at_ms: body.server_sync_at.map(|at| at.timestamp_millis()),
            send_id: body.send_id.clone(),
            sync_id: body.sync_id.clone(),
            msg_type: repr_code(&body.msg_type)?,
            event: repr_code(&body.event)?,
            room_id: body.room_id.clone(),
            uid: body.uid,
            sender_json: body.sender.as_ref().map(serde_json::to_vec).transpose()?,
            payload_kind,
            payload_text,
            payload_json,
            channel: body.channel.as_ref().map(repr_code).transpose()?,
            recipient_uid: body.recipient_uid,
            ack_message_id: body.ack_message_id.clone(),
            device_id: body.device_id.clone(),
            ack_stage: body.ack_stage.as_ref().map(ack_stage_name).transpose()?,
        })
    }

    /// # [HANDLER] - into_body
    /// * `DESC`: `将 protobuf 线上结构还原为领域消息体，非法字段返回错误`
    fn into_body(self) -> Result<MessageBody, CodecError> {
        let payload = match self.payload_kind {
            PAYLOAD_SYSTEM => Payload::System {
                message: self.payload_text,
            },
            PAYLOAD_CONTENT => Payload::Content {
                text: self.payload_text,
            },
            PAYLOAD_JSON => serde_json::from_slice::<Payload>(&self.payload_json)?,
            other => {
                return Err(CodecError::ProtoField(format!(
                    "unknown payload kind {other}"
                )))
            }
        };
        Ok(MessageBody {
            status: repr_from_code(self.status)?,
            client_id: self.client_id,
            send_at: timestamp(self.send_at_ms)?,
            server_sync_at: self.server_sync_at_ms.map(timestamp).transpose()?,
            send_id: self.send_id,
            sync_id: self.sync_id,
            msg_type: repr_from_code(self.msg_type)?,
            event: repr_from_code(self.event)?,
            room_id: self.room_id,
            uid: self.uid,
            sender: self
                .sender_json
                .as_deref()
                .map(serde_json::from_slice)
                .transpose()?,
            payload,
            channel: self.channel.map(repr_from_code).transpose()?,
            recipient_uid: self.recipient_uid,
            ack_message_id: self.ack_message_id,
            device_id: self.device_id,
            ack_stage: self
                .ack_stage
                .as_deref()
                .map(ack_stage_from_name)
                .transpose()?,
        })
    }
}

////////

#[cfg(test)]
mod tests {
    use super::{JsonCodec, MessageCodec, ProtobufCodec};
    use crate::models::message::MessageBody;

    /// # [HANDLER] - json_codec_rejects_invalid_message
    /// * `DESC`: `验证 JSON 编解码器拒绝非法消息`
    #[test]
    fn json_codec_rejects_invalid_message() {
        assert!(JsonCodec.decode(b"not-json").is_err());
    }

    /// # [HANDLER] - protobuf_codec_round_trips_message
    /// * `DESC`: `验证 protobuf 编解码往返保持关键字段，并拒绝垃圾字节`
    #[test]
    fn protobuf_codec_round_trips_message() {
        use crate::models::message::{Event, MsgChannel, MsgType, Payload, Status};
        use chrono::DateTime;

        let body = MessageBody {
            status: Status::Sending,
            client_id: "client".to_owned(),
            send_at: DateTime::from_timestamp_millis(1_700_000_000_123).unwrap(),
            server_sync_at: None,
            send_id: "message".to_owned(),
            sync_id: String::new(),
            msg_type: MsgType::Text,
            event: Event::SendMsg,
            room_id: "room".to_owned(),
            uid: 7,
            sender: None,
            payload: Payload::Content {
                text: "hello".to_owned(),
            },
            channel: Some(MsgChannel::single),
            recipient_uid: Some(8),
            ack_message_id: None,
            device_id: None,
            ack_stage: None,
        };

        let codec = ProtobufCodec;
        let bytes = codec.encode(&body).unwrap();
        let decoded = codec.decode(&bytes).unwrap();
        assert_eq!(decoded.room_id, "room");
        assert_eq!(decoded.send_at.timestamp_millis(), 1_700_000_000_123);
        assert_eq!(decoded.channel, Some(MsgChannel::single));
        assert_eq!(decoded.recipient_uid, Some(8));
        assert!(matches!(decoded.payload, Payload::Content { ref text } if text == "hello"));
        assert!(codec.decode(&[0xff, 0xff, 0xff]).is_err());
    }
}

//////// END
