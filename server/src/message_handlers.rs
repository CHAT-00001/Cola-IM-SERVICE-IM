// server/src/message_handlers.rs -- REST handlers for message sync and send
// 2026/10/11 23:46 Created.

////////

use std::sync::Arc;

use actix_web::{web, HttpRequest, HttpResponse};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::chat::model::PendingDelivery;
use crate::chat::port::ChatError;
use crate::models::message::{Event, MessageBody, MsgChannel, MsgType, Payload, Status};
use crate::shared_handler::ChatHandler;

////////

type ChatHandlerData = web::Data<Arc<ChatHandler>>;

////////

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: &'static str,
    message: String,
}

#[derive(Debug, Deserialize)]
pub struct MessageSyncQuery {
    #[serde(default)]
    pub limit: Option<u32>,
    /// 会话 ID：存在时按时间线返回该会话历史，缺省时仍返回未确认的离线消息。
    #[serde(default)]
    pub room_id: Option<String>,
    /// 历史翻页游标：上一页最早一条消息的 `send_at`（RFC3339）。
    #[serde(default)]
    pub before: Option<String>,
}

#[derive(Debug, Serialize)]
struct MessageSyncResponse {
    messages: Vec<PendingDelivery>,
}

#[derive(Debug, Serialize)]
struct MessageHistoryResponse {
    messages: Vec<MessageBody>,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub channel: MsgChannel,
    pub send_id: String,
    pub payload: Payload,
    #[serde(default)]
    pub msg_type: Option<MsgType>,
    #[serde(default)]
    pub recipient_uid: Option<u64>,
    #[serde(default)]
    pub room_id: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
}

////////

/// # [HANDLER] - sync_offline_messages
/// * `DESC`: `验签当前用户并按设备返回 MongoDB 中尚未确认的离线消息`
pub async fn sync_offline_messages(
    request: HttpRequest,
    handler: ChatHandlerData,
    query: web::Query<MessageSyncQuery>,
) -> HttpResponse {
    let session = match crate::session_handlers::authenticated_session(&request) {
        Ok(session) => session,
        Err(response) => return response,
    };
    let limit = query.limit.unwrap_or(50);

    // room_id 存在时按会话时间线取历史（重开会话/刷新页面补消息）；
    // 缺省时保持原有语义：返回该设备尚未确认的离线消息。
    if let Some(room_id) = query
        .room_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let before = match query.before.as_deref().map(str::trim) {
            Some(raw) if !raw.is_empty() => match chrono::DateTime::parse_from_rfc3339(raw) {
                Ok(value) => Some(value.with_timezone(&Utc)),
                Err(_) => {
                    return HttpResponse::BadRequest().json(ErrorResponse {
                        error: "invalid_request",
                        message: "before must be an RFC3339 timestamp".to_owned(),
                    });
                }
            },
            _ => None,
        };
        return match handler
            .conversation_history(&session, room_id, limit, before)
            .await
        {
            Ok(messages) => HttpResponse::Ok().json(MessageHistoryResponse { messages }),
            Err(ChatError::Unauthorized) => {
                HttpResponse::Forbidden().json(ErrorResponse {
                    error: "forbidden",
                    message: "conversation is not accessible".to_owned(),
                })
            }
            Err(error) => error_response(error, "could not load conversation history"),
        };
    }

    match handler.pending_messages_for_device(&session, limit).await {
        Ok(messages) => HttpResponse::Ok().json(MessageSyncResponse { messages }),
        Err(error) => error_response(error, "could not sync offline messages"),
    }
}

////////

/// # [HANDLER] - send_message
/// * `DESC`: `验签当前用户并通过单聊或群聊服务发送一条消息`
pub async fn send_message(
    request: HttpRequest,
    handler: ChatHandlerData,
    body: web::Json<SendMessageRequest>,
) -> HttpResponse {
    let session = match crate::session_handlers::authenticated_session(&request) {
        Ok(session) => session,
        Err(response) => return response,
    };
    let request = body.into_inner();

    if request.send_id.trim().is_empty() {
        return HttpResponse::BadRequest().json(ErrorResponse {
            error: "invalid_request",
            message: "send_id is required".to_owned(),
        });
    }
    if !matches!(request.channel, MsgChannel::single | MsgChannel::group) {
        return HttpResponse::BadRequest().json(ErrorResponse {
            error: "invalid_request",
            message: "channel must be single(1) or group(2)".to_owned(),
        });
    }

    let message = build_message(&session, request);

    match handler.send_authenticated_message(&session, message).await {
        Ok(acknowledgement) => HttpResponse::Ok().json(acknowledgement),
        Err(error) => error_response(error, "message could not be sent"),
    }
}

////////

/// # [HANDLER] - build_message
/// * `DESC`: `将 REST 发送请求转换为内部消息体并补齐默认字段`
fn build_message(session: &crate::chat::model::AuthenticatedSession, request: SendMessageRequest) -> MessageBody {
    MessageBody {
        status: Status::Sending,
        client_id: request.client_id.unwrap_or_default(),
        send_at: Utc::now(),
        server_sync_at: None,
        send_id: request.send_id,
        sync_id: String::new(),
        msg_type: request.msg_type.unwrap_or(MsgType::Text),
        event: Event::SendMsg,
        room_id: request.room_id.unwrap_or_default(),
        uid: session.account_id,
        sender: None,
        payload: request.payload,
        channel: Some(request.channel),
        recipient_uid: request.recipient_uid,
        ack_message_id: None,
        device_id: Some(session.device_id.clone()),
        ack_stage: None,
    }
}

////////

/// # [HANDLER] - error_response
/// * `DESC`: `将聊天错误映射为不泄漏内部细节的 HTTP 响应`
fn error_response(error: ChatError, log_message: &str) -> HttpResponse {
    match error {
        ChatError::InvalidMessage(message) => HttpResponse::BadRequest().json(ErrorResponse {
            error: "invalid_request",
            message,
        }),
        ChatError::Unauthorized => HttpResponse::Forbidden().json(ErrorResponse {
            error: "forbidden",
            message: "message is not allowed".to_owned(),
        }),
        ChatError::MessageNotFound => HttpResponse::NotFound().json(ErrorResponse {
            error: "not_found",
            message: "message was not found".to_owned(),
        }),
        ChatError::Persistence(message) => {
            tracing::error!("{log_message}: {message}");
            HttpResponse::InternalServerError().json(ErrorResponse {
                error: "internal_error",
                message: log_message.to_owned(),
            })
        }
    }
}

//////// END
