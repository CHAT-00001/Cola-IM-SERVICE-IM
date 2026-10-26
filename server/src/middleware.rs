// server/src/middleware.rs -- HTTP request logging middleware
// 2026/10/11 19:54 Created.

////////

use std::future::{ready, Ready};
use std::time::Instant;

use actix_web::body::MessageBody;
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::header;
use actix_web::Error;
use futures_util::future::LocalBoxFuture;

////////

/// # [HANDLER] - clickable_url
/// * `DESC`: `将 URL 渲染为深蓝色带下划线的终端可点击超链接`
fn clickable_url(url: &str) -> String {
    // OSC 8 超链接: ESC ] 8 ; ; <URI> ST <文本> ESC ] 8 ; ; ST
    // 深蓝色(34) + 下划线(4) 由 ANSI SGR 提供，点击跳转由 OSC 8 提供
    format!("\x1b]8;;{url}\x1b\\\x1b[34;4m{url}\x1b[0m\x1b]8;;\x1b\\")
}

////////

/// # [HANDLER] - redact_sensitive_query_values
/// * `DESC`: `HTTP 请求日志中隐藏 ticket 与 token 等敏感查询参数`
fn redact_sensitive_query_values(path_and_query: &str) -> String {
    let Some((path, query)) = path_and_query.split_once('?') else {
        return path_and_query.to_owned();
    };

    let redacted_query = query
        .split('&')
        .map(|pair| {
            let (name, _) = pair.split_once('=').unwrap_or((pair, ""));
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "ticket"
                    | "token"
                    | "access_token"
                    | "refresh_token"
                    | "authorization"
                    | "key"
                    | "signature"
            ) {
                format!("{name}=[REDACTED]")
            } else {
                pair.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("&");

    format!("{path}?{redacted_query}")
}

////////

/// # [HANDLER] - describe_request
/// * `DESC`: `从服务请求中提取可读的方法、URL、来源等日志字段`
fn describe_request(req: &ServiceRequest) -> (String, String, String, String, String) {
    let method = req.method().as_str().to_owned();

    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|value| redact_sensitive_query_values(value.as_str()))
        .unwrap_or_else(|| req.path().to_owned());

    let connection = req.connection_info();
    let full_url = format!(
        "{}://{}{}",
        connection.scheme(),
        connection.host(),
        path_and_query
    );

    let peer = req
        .peer_addr()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|| "-".to_owned());

    let user_agent = req
        .headers()
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-")
        .to_owned();

    (method, full_url, peer, path_and_query, user_agent)
}

////////

/// # [HANDLER] - RequestLogger
/// * `DESC`: `HTTP 请求日志中间件工厂，为每个请求挂载请求日志服务`
pub struct RequestLogger;

////////

impl<S, B> Transform<S, ServiceRequest> for RequestLogger
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = RequestLoggerMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RequestLoggerMiddleware { service }))
    }
}

////////

/// # [HANDLER] - RequestLoggerMiddleware
/// * `DESC`: `记录每个 HTTP 请求的方法、URL、状态码与耗时`
pub struct RequestLoggerMiddleware<S> {
    service: S,
}

////////

impl<S, B> Service<ServiceRequest> for RequestLoggerMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let (method, full_url, peer, _path_and_query, user_agent) = describe_request(&req);
        let started_at = Instant::now();
        let future = self.service.call(req);

        Box::pin(async move {
            let response = future.await?;
            let status = response.status();
            let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
            let url = clickable_url(&full_url);

            tracing::info!(
                target: "http",
                "🌐 [HTTP] {method} {url} -> {status} ({elapsed_ms:.1}ms) | peer={peer} | ua={user_agent}"
            );

            Ok(response)
        })
    }
}

////////

#[cfg(test)]
mod tests {
    use super::{clickable_url, redact_sensitive_query_values};

    /// # [HANDLER] - wraps_url_with_osc8_hyperlink
    /// * `DESC`: `验证 URL 被包裹为深蓝色下划线的 OSC 8 超链接`
    #[test]
    fn wraps_url_with_osc8_hyperlink() {
        let rendered = clickable_url("http://127.0.0.1:19967/chat/session/add");

        assert!(rendered.starts_with("\x1b]8;;http://127.0.0.1:19967/chat/session/add\x1b\\"));
        assert!(rendered.contains("\x1b[34;4m"));
        assert!(rendered.ends_with("\x1b[0m\x1b]8;;\x1b\\"));
    }

    #[test]
    fn redacts_ticket_and_authentication_query_values() {
        assert_eq!(
            redact_sensitive_query_values(
                "/ws?ticket=secret-ticket&room_id=room-1&token=secret-token"
            ),
            "/ws?ticket=[REDACTED]&room_id=room-1&token=[REDACTED]"
        );
    }
}

//////// END
