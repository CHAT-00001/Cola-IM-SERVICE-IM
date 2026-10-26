// server/src/session_handlers.rs -- HTTP handlers for direct chat sessions
// 2026/10/11 17:17 Created.

////////

use actix_web::{web, HttpRequest, HttpResponse};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

use app_core::models::chat::ChatSessionListItem;
use crate::chat::model::AuthenticatedSession;
use crate::chat::port::ChatError;
use crate::chat::session::DirectChatSessionService;

////////

#[derive(Debug, Deserialize)]
struct JwtClaims {
    sub: i64,
    device_id: String,
    exp: usize,
    iat: usize,
    jti: String,
}

#[derive(Debug, Deserialize)]
pub struct AddDirectChatSessionRequest {
    #[serde(alias = "recipient_uid")]
    pub peer_account_id: u64,
}

#[derive(Debug, Serialize)]
struct AddDirectChatSessionResponse {
    session_id: String,
    channel: u8,
    /// 会话对端账号；客户端据此填充发送消息所需的 `recipient_uid`。
    peer_account_id: u64,
    status: i16,
    pinned: bool,
    last_message_summary: Option<String>,
    notification_settings: serde_json::Value,
    member_state: i16,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: &'static str,
    message: String,
}

type SessionService = DirectChatSessionService<
    storage_pg::repository::chat_session::PgDirectChatSessionRepository,
    crate::service::blocklist::AllowAllBlocklist,
>;

/// # [HANDLER] - add_direct_chat_session
/// * `DESC`: `验签当前用户并幂等创建或返回其与目标账号的单聊会话`
pub async fn add_direct_chat_session(
    request: HttpRequest,
    service: web::Data<std::sync::Arc<SessionService>>,
    body: web::Json<AddDirectChatSessionRequest>,
) -> HttpResponse {
    let session = match authenticated_session(&request) {
        Ok(session) => session,
        Err(response) => return response,
    };

    match service
        .ensure_session(session.account_id, body.peer_account_id)
        .await
    {
        Ok(chat_session) => HttpResponse::Ok().json(AddDirectChatSessionResponse {
            session_id: chat_session.session_id,
            channel: chat_session.channel,
            peer_account_id: body.peer_account_id,
            status: chat_session.status,
            pinned: chat_session.pinned,
            last_message_summary: chat_session.last_message_summary,
            notification_settings: chat_session.notification_settings,
            member_state: chat_session.member_state,
        }),
        Err(ChatError::InvalidMessage(message)) => HttpResponse::BadRequest().json(ErrorResponse {
            error: "invalid_request",
            message,
        }),
        Err(ChatError::Unauthorized) => HttpResponse::Forbidden().json(ErrorResponse {
            error: "forbidden",
            message: "direct chat session is not allowed".to_owned(),
        }),
        Err(ChatError::Persistence(message)) => {
            tracing::error!("Failed to create direct chat session: {message}");
            HttpResponse::InternalServerError().json(ErrorResponse {
                error: "internal_error",
                message: "could not create chat session".to_owned(),
            })
        }
        Err(ChatError::MessageNotFound) => {
            HttpResponse::InternalServerError().json(ErrorResponse {
                error: "internal_error",
                message: "unexpected chat session error".to_owned(),
            })
        }
    }
}

pub(crate) fn authenticated_session(
    request: &HttpRequest,
) -> Result<AuthenticatedSession, HttpResponse> {
    let token = request
        .headers()
        .get(actix_web::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            let (scheme, token) = value.split_once(' ')?;
            scheme.eq_ignore_ascii_case("Bearer").then_some(token)
        })
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            HttpResponse::Unauthorized().json(ErrorResponse {
                error: "unauthorized",
                message: "a Bearer access token is required".to_owned(),
            })
        })?;

    let secret = std::env::var("COLA_JWT_SECRET")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            tracing::error!("COLA_JWT_SECRET is not configured (missing or empty)");
            HttpResponse::ServiceUnavailable().json(ErrorResponse {
                error: "auth_unavailable",
                message: "authentication is not configured".to_owned(),
            })
        })?;

    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_aud = false;
    let claims = decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|_| {
        HttpResponse::Unauthorized().json(ErrorResponse {
            error: "unauthorized",
            message: "invalid or expired access token".to_owned(),
        })
    })?
    .claims;

    if claims.sub <= 0 || claims.device_id.trim().is_empty() || claims.jti.trim().is_empty() {
        return Err(HttpResponse::Unauthorized().json(ErrorResponse {
            error: "unauthorized",
            message: "access token is missing required identity claims".to_owned(),
        }));
    }
    let _issued_at = claims.iat;
    let _expires_at = claims.exp;

    Ok(AuthenticatedSession {
        account_id: u64::try_from(claims.sub).map_err(|_| {
            HttpResponse::Unauthorized().json(ErrorResponse {
                error: "unauthorized",
                message: "invalid account id in access token".to_owned(),
            })
        })?,
        device_id: claims.device_id,
        connection_id: String::new(),
    })
}

////////

#[derive(Debug, Serialize)]
struct ChatSessionListResponse {
    sessions: Vec<ChatSessionListItem>,
}

/// # [HANDLER] - list_direct_chat_sessions
/// * `DESC`: `验签当前用户并返回其参与的聊天会话列表`
pub async fn list_direct_chat_sessions(
    request: HttpRequest,
    service: web::Data<std::sync::Arc<SessionService>>,
) -> HttpResponse {
    let session = match authenticated_session(&request) {
        Ok(session) => session,
        Err(response) => return response,
    };

    match service.list_sessions(session.account_id).await {
        Ok(sessions) => HttpResponse::Ok().json(ChatSessionListResponse { sessions }),
        Err(ChatError::InvalidMessage(message)) => HttpResponse::BadRequest().json(ErrorResponse {
            error: "invalid_request",
            message,
        }),
        Err(ChatError::Unauthorized) => HttpResponse::Forbidden().json(ErrorResponse {
            error: "forbidden",
            message: "chat sessions are not allowed".to_owned(),
        }),
        Err(ChatError::Persistence(message)) => {
            tracing::error!("Failed to list direct chat sessions: {message}");
            HttpResponse::InternalServerError().json(ErrorResponse {
                error: "internal_error",
                message: "could not list chat sessions".to_owned(),
            })
        }
        Err(ChatError::MessageNotFound) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: "internal_error",
            message: "unexpected chat session error".to_owned(),
        }),
    }
}

//////// END
