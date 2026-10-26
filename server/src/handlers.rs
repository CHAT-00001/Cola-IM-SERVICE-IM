// server/src/handlers.rs -- WebSocket transport adapter
// 2026-01-05 11:20:00

use std::sync::Arc;

use actix::{Actor, ActorContext, AsyncContext, StreamHandler};
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{info, warn};
use uuid::Uuid;

use crate::chat::model::AuthenticatedSession;
use crate::chat::ticket::TicketStore;
use crate::codec::{JsonCodec, MessageCodec};
use crate::config::WsSettings;
use crate::hub::ChatHub;
use crate::models::message::{MessageBody, MsgChannel};
use crate::shared_handler::ChatHandler;

pub struct ChatSession {
    connection_id: String,
    peer: String,
    handler: Arc<ChatHandler>,
    initial_room_id: Option<String>,
    hub: ChatHub,
    max_message_bytes: usize,
    authenticated_identity: Option<(u64, String)>,
}

impl Actor for ChatSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!(
            peer = %self.peer,
            connection_id = %self.connection_id,
            authenticated = self.authenticated_identity.is_some(),
            "WebSocket connection established"
        );
        let receiver = self.hub.register(self.connection_id.clone());
        if let Some((account_id, device_id)) = &self.authenticated_identity {
            if !self
                .hub
                .register_authenticated_device(&self.connection_id, *account_id, device_id)
            {
                warn!(
                    "Connection {} authentication binding failed",
                    self.connection_id
                );
                ctx.close(Some(ws::CloseReason {
                    code: ws::CloseCode::Policy,
                    description: Some("authentication binding failed".to_owned()),
                }));
                ctx.stop();
                return;
            }
        }
        if let Some(room_id) = self.initial_room_id.take() {
            if !self.handler.hub().join_room(
                &self.connection_id,
                &room_id,
                self.handler.max_rooms_per_connection(),
            ) {
                warn!(
                    "Connection {} could not join initial room",
                    self.connection_id
                );
            }
        }

        ctx.add_stream(ReceiverStream::new(receiver));
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!(
            peer = %self.peer,
            connection_id = %self.connection_id,
            "WebSocket connection stopped"
        );
        self.hub.unregister(&self.connection_id);
    }
}

impl StreamHandler<MessageBody> for ChatSession {
    fn handle(&mut self, message: MessageBody, ctx: &mut Self::Context) {
        match JsonCodec.encode(&message) {
            Ok(encoded) => ctx.text(encoded),
            Err(error) => warn!("Failed to encode outbound WebSocket message: {error}"),
        }
    }

    fn finished(&mut self, ctx: &mut Self::Context) {
        ctx.stop();
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ChatSession {
    fn handle(&mut self, message: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match message {
            Ok(ws::Message::Text(text)) => {
                if text.len() > self.max_message_bytes {
                    warn!(
                        "Connection {} sent an oversized WebSocket message",
                        self.connection_id
                    );
                    ctx.close(Some(ws::CloseReason {
                        code: ws::CloseCode::Size,
                        description: Some("message exceeds configured limit".to_owned()),
                    }));
                    ctx.stop();
                    return;
                }

                match JsonCodec.decode(&text) {
                    Ok(message) => match message.channel.unwrap_or(MsgChannel::public) {
                        MsgChannel::single | MsgChannel::group | MsgChannel::received => {
                            let Some((account_id, device_id)) = &self.authenticated_identity else {
                                let channel = message.channel.unwrap_or(MsgChannel::public);
                                self.handler.handle_unauthenticated_direct_or_group(channel);
                                return;
                            };
                            let session = AuthenticatedSession {
                                account_id: *account_id,
                                device_id: device_id.clone(),
                                connection_id: self.connection_id.clone(),
                            };
                            let handler = Arc::clone(&self.handler);
                            let hub = self.hub.clone();
                            let connection_id = self.connection_id.clone();
                            let channel = message.channel.unwrap_or(MsgChannel::public);
                            actix::spawn(async move {
                                if let Err(error) = handler
                                    .handle_authenticated_message(&session, message)
                                    .await
                                {
                                    warn!("Authenticated {channel:?} message failed: {error}");
                                    let error_message =
                                        crate::shared_handler::protocol_error_message(
                                            error, channel,
                                        );
                                    if let Some(error_message) = error_message {
                                        if let Err(queue_error) =
                                            hub.enqueue_to_connection(&connection_id, error_message)
                                        {
                                            warn!("Failed to queue direct-message error: {queue_error}");
                                        }
                                    }
                                }
                            });
                        }
                        _ => self.handler.handle_message(&self.connection_id, message),
                    },
                    Err(error) => {
                        warn!("Invalid JSON message from {}: {error}", self.connection_id)
                    }
                }
            }
            Ok(ws::Message::Ping(payload)) => ctx.pong(&payload),
            Ok(ws::Message::Close(reason)) => {
                info!(
                    peer = %self.peer,
                    connection_id = %self.connection_id,
                    close = ?reason,
                    "WebSocket client sent close frame"
                );
                ctx.close(reason);
                ctx.stop();
            }
            Err(error) => {
                warn!(
                    "WebSocket protocol error from {}: {error}",
                    self.connection_id
                );
                ctx.stop();
            }
            _ => {}
        }
    }
}

/// # [HANDLER] - ws_index
/// * `DESC`: `鎺ュ彈 WebSocket 鎻℃墜骞跺皢杩炴帴浜ょ粰鍏变韩鑱婂ぉ Handler`
pub async fn ws_index(
    req: HttpRequest,
    stream: web::Payload,
    handler: web::Data<Arc<ChatHandler>>,
    hub: web::Data<ChatHub>,
    ws_settings: web::Data<WsSettings>,
    tickets: Option<web::Data<Arc<TicketStore>>>,
) -> Result<HttpResponse, Error> {
    let peer = req
        .peer_addr()
        .map(|address| address.to_string())
        .unwrap_or_else(|| "-".to_owned());
    let query: std::collections::HashMap<String, String> =
        serde_urlencoded::from_str(req.query_string()).unwrap_or_default();
    let initial_room_id = query
        .get("room_id")
        .filter(|room_id| !room_id.is_empty())
        .cloned();
    let ticket = query.get("ticket").filter(|value| !value.is_empty());
    let has_authorization = req
        .headers()
        .contains_key(actix_web::http::header::AUTHORIZATION);

    // 不记录 query string：其中的 ticket 是一次性敏感凭证。
    info!(
        method = %req.method(),
        path = %req.path(),
        peer = %peer,
        ticket_present = ticket.is_some(),
        authorization_present = has_authorization,
        "WebSocket handshake request"
    );

    // 身份来源互斥：票据与 Authorization 头同时出现时拒绝，避免两种身份冲突。
    if ticket.is_some() && has_authorization {
        warn!(peer = %peer, status = 400, reason = "ticket and Authorization header combined", "WebSocket handshake rejected");
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "invalid_request",
            "message": "ticket and Authorization header cannot be combined",
        })));
    }

    let authenticated_identity = if let Some(ticket) = ticket {
        let Some(store) = tickets else {
            warn!(peer = %peer, status = 503, reason = "ticket authentication is not configured", "WebSocket handshake rejected");
            return Ok(HttpResponse::ServiceUnavailable().json(serde_json::json!({
                "error": "auth_unavailable",
                "message": "ticket authentication is not configured",
            })));
        };
        match store.consume(ticket) {
            Some(session) => Some((session.account_id, session.device_id)),
            None => {
                // 不记录完整票据，仅记录客户端、状态码与拒绝原因。
                warn!(peer = %peer, status = 401, reason = "ticket expired, reused or unknown", "WebSocket handshake rejected");
                return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                    "error": "unauthorized",
                    "message": "invalid or expired websocket ticket",
                })));
            }
        }
    } else if has_authorization {
        match crate::session_handlers::authenticated_session(&req) {
            Ok(session) => Some((session.account_id, session.device_id)),
            Err(response) => {
                warn!(peer = %peer, status = %response.status(), reason = "authorization rejected", "WebSocket handshake rejected");
                return Ok(response);
            }
        }
    } else {
        None
    };
    let connection_id = Uuid::new_v4().to_string();
    let authenticated = authenticated_identity.is_some();

    let session = ChatSession {
        connection_id: connection_id.clone(),
        peer: peer.clone(),
        handler: Arc::clone(handler.get_ref()),
        initial_room_id,
        hub: hub.get_ref().clone(),
        max_message_bytes: ws_settings.max_message_bytes,
        authenticated_identity,
    };

    match ws::start(session, &req, stream) {
        Ok(response) => {
            info!(
                peer = %peer,
                connection_id = %connection_id,
                status = %response.status(),
                authenticated,
                "WebSocket handshake response"
            );
            Ok(response)
        }
        Err(error) => {
            warn!(
                peer = %peer,
                connection_id = %connection_id,
                error = %error,
                "WebSocket handshake failed"
            );
            Err(error)
        }
    }
}
