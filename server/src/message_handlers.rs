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
}

#[derive(Debug, Serialize)]
struct MessageSyncResponse {
    messages: Vec<PendingDelivery>,
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
