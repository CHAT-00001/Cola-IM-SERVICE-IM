// server/src/ticket_handlers.rs -- HTTP handler that issues one-time WebSocket tickets
// 2026/10/25 23:41 Created.

////////

use std::sync::Arc;

use actix_web::{web, HttpRequest, HttpResponse};
use serde::Serialize;

use crate::chat::ticket::TicketStore;

////////

#[derive(Debug, Serialize)]
struct IssueTicketResponse {
    ticket: String,
    expires_in_seconds: u64,
}

////////

/// # [HANDLER] - issue_ws_ticket
/// * `DESC`: `验签 Bearer JWT 后签发一次性 WebSocket 握手票据，票据不回显 JWT`
pub async fn issue_ws_ticket(
    request: HttpRequest,
    tickets: web::Data<Arc<TicketStore>>,
) -> HttpResponse {
    let session = match crate::session_handlers::authenticated_session(&request) {
        Ok(session) => session,
        Err(response) => return response,
    };

    let ticket = tickets.issue(&session);
    HttpResponse::Ok().json(IssueTicketResponse {
        ticket,
        expires_in_seconds: tickets.ttl_seconds(),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use actix_web::{http::StatusCode, test, web, App};

    use crate::chat::ticket::TicketStore;

    /// # [HANDLER] - ticket_route_is_served_on_main_app
    /// * `DESC`: `验证票据接口与 /ws 挂载在同一 App 上，无需独立端口`
    #[actix_web::test]
    async fn ticket_route_is_served_on_main_app() {
        let tickets = web::Data::new(Arc::new(TicketStore::new(Duration::from_secs(30))));
        let app = test::init_service(
            App::new()
                .app_data(tickets.clone())
                .route(
                    "/ws/ticket",
                    web::post().to(super::issue_ws_ticket),
                ),
        )
        .await;

        // 未携带 Bearer JWT 时应返回 401（路由存在），而不是 404（路由缺失）。
        let request = test::TestRequest::post().uri("/ws/ticket").to_request();
        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}

//////// END
