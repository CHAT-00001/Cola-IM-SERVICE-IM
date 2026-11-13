// server/src/wt.rs -- WebTransport transport adapter (quinn + h3-webtransport)
// 2026/10/27 Created.

////////

use std::net::SocketAddr;
use std::sync::Arc;

use quinn::crypto::rustls::QuicServerConfig;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};

use h3_webtransport::server::AcceptedBi;
use h3_webtransport::server::WebTransportSession;

use crate::chat::model::AuthenticatedSession;
use crate::chat::ticket::TicketStore;
use crate::codec::{MessageCodec, WireFormat};
use crate::config::WtSettings;
use crate::e2ee::E2eeHook;
use crate::hub::ChatHub;
use crate::shared_handler::ChatHandler;

////////

/// # [HANDLER] - WtDeps
/// * `DESC`: `WT 适配器依赖的共享服务，与 WS 适配器复用同一份 hub、handler 与票据表`
pub struct WtDeps {
    pub tickets: Arc<TicketStore>,
    pub handler: Arc<ChatHandler>,
    pub hub: ChatHub,
    pub e2ee: Arc<dyn E2eeHook>,
    pub max_frame_bytes: usize,
}

////////

/// # [HANDLER] - build_endpoint
/// * `DESC`: `依据配置加载 TLS 证书与私钥，构建绑定到 WT 端口的 quinn 服务端 Endpoint。
///   证书或私钥缺失与格式错误直接返回错误，不静默降级。`
pub fn build_endpoint(settings: &WtSettings) -> Result<quinn::Endpoint, String> {
    let cert_path = settings
        .cert_file
        .as_deref()
        .filter(|path| !path.trim().is_empty())
        .ok_or("server.wt.cert_file is required when WebTransport is enabled")?;
    let key_path = settings
        .key_file
        .as_deref()
        .filter(|path| !path.trim().is_empty())
        .ok_or("server.wt.key_file is required when WebTransport is enabled")?;

    let certs = CertificateDer::pem_file_iter(cert_path)
        .map_err(|error| format!("failed to read cert_file {cert_path}: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("invalid certificate in {cert_path}: {error}"))?;
    if certs.is_empty() {
        return Err(format!("no certificate found in {cert_path}"));
    }
    let key = PrivateKeyDer::from_pem_file(key_path)
        .map_err(|error| format!("failed to read key_file {key_path}: {error}"))?;

    let mut tls = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .map_err(|error| format!("invalid TLS protocol versions: {error}"))?
    .with_no_client_auth()
    .with_single_cert(certs, key)
    .map_err(|error| format!("invalid certificate or key: {error}"))?;
    // WebTransport over HTTP/3 必须使用 h3 ALPN。
    tls.alpn_protocols = vec![b"h3".to_vec()];

    let crypto = QuicServerConfig::try_from(tls)
        .map_err(|error| format!("failed to build QUIC crypto config: {error}"))?;
    let server_config = quinn::ServerConfig::with_crypto(Arc::new(crypto));

    let addr: SocketAddr = format!("{}:{}", settings.host, settings.port)
        .parse()
        .map_err(|error| format!("invalid server.wt address: {error}"))?;
    quinn::Endpoint::server(server_config, addr)
        .map_err(|error| format!("failed to bind WebTransport endpoint {addr}: {error}"))
}

////////

/// # [HANDLER] - WtQuery
/// * `DESC`: `WT CONNECT 请求的查询参数：ticket 用于鉴权，format 用于协商线上数据格式`
#[derive(Debug, Default)]
pub struct WtQuery {
    pub ticket: Option<String>,
    pub format: Option<String>,
}

////////

/// # [HANDLER] - parse_query
/// * `DESC`: `解析 CONNECT 请求 URI 的查询参数，缺省值为空`
pub fn parse_query(uri: &http::Uri) -> WtQuery {
    let pairs: std::collections::HashMap<String, String> = uri
        .query()
        .and_then(|query| serde_urlencoded::from_str(query).ok())
        .unwrap_or_default();
    WtQuery {
        ticket: pairs.get("ticket").filter(|v| !v.is_empty()).cloned(),
        format: pairs.get("format").filter(|v| !v.is_empty()).cloned(),
    }
}

////////

#[cfg(test)]
mod tests {
    use super::{build_endpoint, parse_query, WtQuery};
    use crate::config::WtSettings;

    /// # [HANDLER] - build_endpoint_rejects_missing_cert
    /// * `DESC`: `验证未配置证书时拒绝启动，不静默降级`
    #[test]
    fn build_endpoint_rejects_missing_cert() {
        let settings = WtSettings {
            host: "127.0.0.1".to_owned(),
            port: 0,
            enabled: true,
            cert_file: None,
            key_file: None,
            allow_insecure_passthrough: false,
        };
        assert!(build_endpoint(&settings).is_err());
    }

    /// # [HANDLER] - build_endpoint_binds_with_valid_cert
    /// * `DESC`: `使用本地生成的 ECDSA 自签证书验证 TLS 加载与 UDP 绑定成功（需先生成 target/wt-test）`
    #[tokio::test]
    async fn build_endpoint_binds_with_valid_cert() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("target")
            .join("wt-test");
        let cert = dir.join("cert.pem");
        let key = dir.join("key.pem");
        if !cert.exists() || !key.exists() {
            eprintln!("skip: {} missing", dir.display());
            return;
        }
        let settings = WtSettings {
            host: "127.0.0.1".to_owned(),
            port: 0,
            enabled: true,
            cert_file: Some(cert.to_string_lossy().into_owned()),
            key_file: Some(key.to_string_lossy().into_owned()),
            allow_insecure_passthrough: true,
        };
        let endpoint = build_endpoint(&settings).expect("endpoint should bind");
        assert!(endpoint.local_addr().is_ok());
    }

    /// # [HANDLER] - parse_query_extracts_ticket_and_format
    /// * `DESC`: `验证 CONNECT 查询参数中的 ticket 与 format 可被解析`
    #[test]
    fn parse_query_extracts_ticket_and_format() {
        let uri: http::Uri = "https://localhost/wt?ticket=abc&format=protobuf"
            .parse()
            .unwrap();
        let query: WtQuery = parse_query(&uri);
        assert_eq!(query.ticket.as_deref(), Some("abc"));
        assert_eq!(query.format.as_deref(), Some("protobuf"));
    }

    /// # [HANDLER] - frame_round_trips_length_prefix
    /// * `DESC`: `验证长度前缀帧的写入与读取往返一致，并拒绝超限帧`
    #[tokio::test]
    async fn frame_round_trips_length_prefix() {
        let (mut client, mut server) = tokio::io::duplex(1024);
        super::write_frame(&mut client, b"hello").await.unwrap();
        let frame = super::read_frame(&mut server, 1024).await.unwrap();
        assert_eq!(frame.as_deref(), Some(&b"hello"[..]));

        super::write_frame(&mut client, b"toolong").await.unwrap();
        assert!(super::read_frame(&mut server, 3).await.is_err());
    }
}

//////// END

/// # [HANDLER] - MAX_FRAME_BYTES_DEFAULT
/// * `DESC`: `单帧最大字节数的兜底值，实际上限取 ws 配置的 max_message_bytes`
pub const MAX_FRAME_BYTES_DEFAULT: usize = 1024 * 1024;

/// # [HANDLER] - read_frame
/// * `DESC`: `从可靠流读取一个 u32 大端长度前缀帧。长度超过上限返回错误，EOF 返回 None。`
pub async fn read_frame<R>(reader: &mut R, max_bytes: usize) -> std::io::Result<Option<Vec<u8>>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;

    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > max_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("frame of {len} bytes exceeds limit {max_bytes}"),
        ));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await?;
    Ok(Some(body))
}

/// # [HANDLER] - write_frame
/// * `DESC`: `向可靠流写入一个 u32 大端长度前缀帧`
pub async fn write_frame<W>(writer: &mut W, body: &[u8]) -> std::io::Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;

    let len = u32::try_from(body.len())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "frame too large"))?;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(body).await?;
    writer.flush().await
}

/// # [HANDLER] - serve_stream
/// * `DESC`: `处理单条 WT 双向流：循环读取长度前缀帧，经共享入口处理后，将确认按同一 codec 编码、
///   E2EE 加密并回写。帧解析或鉴权失败时结束该流，不影响同一会话的其他流。`
pub async fn serve_stream<S>(
    stream: S,
    deps: Arc<WtDeps>,
    identity: Arc<crate::chat::model::AuthenticatedSession>,
    codec: Arc<dyn MessageCodec>,
) -> Result<(), String>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
{
    let (mut reader, mut writer) = tokio::io::split(stream);
    let session = AuthenticatedSession {
        account_id: identity.account_id,
        device_id: identity.device_id.clone(),
        connection_id: identity.connection_id.clone(),
    };

    while let Some(frame) = read_frame(&mut reader, deps.max_frame_bytes)
        .await
        .map_err(|error| format!("frame read failed: {error}"))?
    {
        let ack = deps
            .handler
            .handle_inbound_frame(&session, codec.as_ref(), deps.e2ee.as_ref(), &frame)
            .await;
        match ack {
            Ok(acknowledgement) => {
                let body = crate::shared_handler::acknowledgement_message(&acknowledgement);
                let encoded = codec
                    .encode(&body)
                    .map_err(|error| format!("ack encode failed: {error}"))?;
                let sealed = deps
                    .e2ee
                    .seal(&session, &encoded)
                    .map_err(|error| format!("ack seal failed: {error}"))?;
                write_frame(&mut writer, &sealed)
                    .await
                    .map_err(|error| format!("frame write failed: {error}"))?;
            }
            Err(error) => {
                tracing::warn!("WebTransport frame rejected: {error}");
                let channel = crate::models::message::MsgChannel::public;
                if let Some(error_body) =
                    crate::shared_handler::protocol_error_message(error, channel)
                {
                    let encoded = codec
                        .encode(&error_body)
                        .map_err(|error| format!("error encode failed: {error}"))?;
                    let sealed = deps
                        .e2ee
                        .seal(&session, &encoded)
                        .map_err(|error| format!("error seal failed: {error}"))?;
                    write_frame(&mut writer, &sealed)
                        .await
                        .map_err(|error| format!("frame write failed: {error}"))?;
                }
            }
        }
    }
    Ok(())
}

/// # [HANDLER] - serve_connection
/// * `DESC`: `处理单个 QUIC 连接：HTTP/3 握手 → CONNECT 校验 → ticket 鉴权 → 建立 WT 会话，
///   随后按双向流逐个交给 serve_stream 处理。鉴权失败直接拒绝该 CONNECT。`
pub async fn serve_connection(incoming: quinn::Incoming, deps: Arc<WtDeps>) -> Result<(), String> {
    let quinn_conn = incoming
        .await
        .map_err(|error| format!("QUIC handshake failed: {error}"))?;
    let h3_conn = h3_quinn::Connection::new(quinn_conn);
    let mut conn: h3::server::Connection<h3_quinn::Connection, bytes::Bytes> =
        h3::server::builder()
            .enable_webtransport(true)
            .enable_extended_connect(true)
            .enable_datagram(true)
            .max_webtransport_sessions(1)
            .build(h3_conn)
            .await
            .map_err(|error| format!("HTTP/3 setup failed: {error}"))?;

    let resolver = conn
        .accept()
        .await
        .map_err(|error| format!("HTTP/3 accept failed: {error}"))?
        .ok_or("connection closed before CONNECT")?;
    let (request, stream) = resolver
        .resolve_request()
        .await
        .map_err(|error| format!("CONNECT resolve failed: {error}"))?;

    let query = parse_query(request.uri());
    let mut identity = match query
        .ticket
        .as_deref()
        .and_then(|t| deps.tickets.consume(t))
    {
        Some(session) => session,
        None => {
            tracing::warn!("WebTransport CONNECT rejected: missing or invalid ticket");
            return Ok(());
        }
    };

    let codec = match query.format.as_deref() {
        None => WireFormat::Json,
        Some(value) => {
            WireFormat::parse(value).ok_or_else(|| format!("unsupported format: {value}"))?
        }
    }
    .codec();

    let session = WebTransportSession::accept(request, stream, conn)
        .await
        .map_err(|error| format!("WebTransport session setup failed: {error}"))?;
    let session = Arc::new(session);
    tracing::info!(
        account_id = identity.account_id,
        "WebTransport session established"
    );

    // 与 WS 一致：分配连接 ID 并绑定已认证设备，使 Hub 可向该连接下行推送。
    let connection_id = uuid::Uuid::new_v4().to_string();
    let receiver = deps.hub.register(connection_id.clone());
    if !deps.hub.register_authenticated_device(
        &connection_id,
        identity.account_id,
        &identity.device_id,
    ) {
        deps.hub.unregister(&connection_id);
        return Err("authentication binding failed".to_owned());
    }
    identity.connection_id = connection_id.clone();
    let identity = Arc::new(identity);
    let codec: Arc<dyn MessageCodec> = Arc::from(codec);
    let session_id = session.session_id();
    let mut receiver = receiver;
    let result: Result<(), String> = async {
        loop {
            tokio::select! {
                accepted = session.accept_bi() => {
                    let accepted = accepted.map_err(|error| format!("accept_bi failed: {error}"))?;
                    let Some(accepted) = accepted else { break };
                    if let AcceptedBi::BidiStream(_, stream) = accepted {
                        let deps = Arc::clone(&deps);
                        let identity = Arc::clone(&identity);
                        let codec = Arc::clone(&codec);
                        tokio::spawn(async move {
                            if let Err(error) = serve_stream(stream, deps, identity, codec).await {
                                tracing::warn!("WebTransport stream ended: {error}");
                            }
                        });
                    }
                }
                message = receiver.recv() => {
                    let Some(message) = message else { break };
                    let encoded = codec
                        .encode(&message)
                        .map_err(|error| format!("outbound encode failed: {error}"))?;
                    let sealed = deps
                        .e2ee
                        .seal(&identity, &encoded)
                        .map_err(|error| format!("outbound seal failed: {error}"))?;
                    let mut stream = session
                        .open_uni(session_id)
                        .await
                        .map_err(|error| format!("open outbound WebTransport stream failed: {error}"))?;
                    write_frame(&mut stream, &sealed)
                        .await
                        .map_err(|error| format!("outbound frame write failed: {error}"))?;
                }
            }
        }
        Ok(())
    }
    .await;
    deps.hub.unregister(&connection_id);
    result
}
