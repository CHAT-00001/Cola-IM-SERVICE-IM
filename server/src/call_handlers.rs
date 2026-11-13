// server/src/call_handlers.rs -- 通话信令 REST 入口（端到端 WebRTC）
// 2026-11-12 17:05 Created.
//
// 设计：信令走 HTTP（邀请/接听/拒接/取消/挂断/ICE），服务端只维护状态机并通过 WS 队列
//       把事件推给对方所有在线设备；媒体不经过服务器（P2P）。
// 超时：邀请后 60s 未接听 → 服务端判定 missed，推送 CallTimeout 并清理。

////////

use std::sync::Arc;
use std::time::Duration;

use actix_web::{HttpRequest, HttpResponse, web};
use chrono::Utc;
use serde::Deserialize;

use crate::call::media::MediaMode;
use crate::call::model::{CallKind, CallRuntime, CallState, EndReason};
use crate::call::registry::CallRegistry;
use crate::chat::port::DeviceConnectionRegistry;
use crate::models::message::{CallInfo, Event, MessageBody, MsgChannel, MsgType, Payload, Status};
use crate::session_handlers::authenticated_session;
use crate::shared_handler::ChatHandler;

////////

type ChatHandlerData = web::Data<Arc<ChatHandler>>;
type RegistryData = web::Data<Arc<CallRegistry>>;

/// 振铃超时（秒）
const RING_TIMEOUT_SECS: u64 = 60;

////////

#[derive(Debug, Deserialize)]
pub struct InviteRequest {
    pub room_id: String,
    pub callee_uid: u64,
    pub kind: String,
    /// 主叫的 offer SDP（P2P 模式必填）
    #[serde(default)]
    pub offer_sdp: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AnswerRequest {
    pub call_id: String,
    pub answer_sdp: String,
}

#[derive(Debug, Deserialize)]
pub struct IceRequest {
    pub call_id: String,
    pub candidate: String,
    /// 候选要发给谁：caller / callee
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub struct CallIdRequest {
    pub call_id: String,
}

////////

// # [HELPER] - 构造信令消息
fn signal(
    event: Event,
    uid: u64,
    recipient: u64,
    room_id: &str,
    info: CallInfo,
) -> MessageBody {
    MessageBody {
        status: Status::Sending,
        client_id: "call".to_owned(),
        send_at: Utc::now(),
        server_sync_at: None,
        send_id: uuid::Uuid::new_v4().to_string(),
        sync_id: String::new(),
        msg_type: MsgType::VideoCall,
        event,
        room_id: room_id.to_owned(),
        uid,
        sender: None,
        payload: Payload::Call(info),
        // 信令走通知通道，客户端按 event 分发，不进入聊天记录
        channel: Some(MsgChannel::Notice),
        recipient_uid: Some(recipient),
        ack_message_id: None,
        device_id: None,
        ack_stage: None,
    }
}

// # [HELPER] - 推给某个账号的所有在线设备
async fn push_to_account(handler: &ChatHandler, account: u64, message: &MessageBody) {
    push_to_account_excluding(handler, account, message, None).await;
}

// # [HELPER] - 推给某个账号的在线设备（可排除指定设备）
// * `DESC`: `接听者自己的设备不需要再收一次 CallState，否则会把来电窗误收起`
async fn push_to_account_excluding(
    handler: &ChatHandler,
    account: u64,
    message: &MessageBody,
    exclude_device: Option<&str>,
) {
    let devices = handler.hub().online_device_ids(account).await;
    tracing::info!(
        event = ?message.event,
        account,
        device_count = devices.len(),
        "call signal push"
    );
    for device in devices {
        if exclude_device == Some(device.as_str()) {
            continue;
        }
        if let Err(error) = handler.hub().enqueue_to_device(account, &device, message).await {
            tracing::warn!(
                event = ?message.event,
                account,
                device = %device,
                "call signal delivery failed: {error}"
            );
        }
    }
}

// # [HELPER] - 通话信息
fn call_info(
    call_id: &str,
    kind: CallKind,
    state: &str,
    reason: Option<EndReason>,
    duration_ms: u64,
    sdp: Option<String>,
    candidate: Option<String>,
) -> CallInfo {
    CallInfo {
        call_id: call_id.to_owned(),
        kind: kind.as_str().to_owned(),
        state: state.to_owned(),
        end_reason: reason.map(|value| value.as_str().to_owned()),
        duration_ms: (duration_ms > 0).then_some(duration_ms),
        media_mode: Some(MediaMode::P2p.as_str().to_owned()),
        sdp,
        candidate,
    }
}

////////

/// # [HANDLER] - invite
/// * `DESC`: `发起呼叫：登记状态机、推送给被叫所有设备、启动 60s 超时任务`
pub async fn invite(
    request: HttpRequest,
    handler: ChatHandlerData,
    registry: RegistryData,
    body: web::Json<InviteRequest>,
) -> HttpResponse {
    let session = match authenticated_session(&request) {
        Ok(session) => session,
        Err(response) => return response,
    };

    let Some(kind) = CallKind::parse(body.kind.trim()) else {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "invalid_request", "message": "kind must be voice or video"
        }));
    };
    if body.callee_uid == session.account_id {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "invalid_request", "message": "cannot call yourself"
        }));
    }
    if body.room_id.trim().is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "invalid_request", "message": "room_id is required"
        }));
    }

    let call_id = uuid::Uuid::new_v4().to_string();
    let runtime = CallRuntime {
        call_id: call_id.clone(),
        room_id: body.room_id.trim().to_owned(),
        caller_uid: session.account_id,
        callee_uid: body.callee_uid,
        caller_device: session.device_id.clone(),
        offer_sdp: body.offer_sdp.clone(),
        kind,
        state: CallState::Ringing,
        created_at: Utc::now(),
        answered_at: None,
        end_reason: None,
    };
    if let Err(error) = registry.create(runtime.clone()).await {
        return HttpResponse::Conflict().json(serde_json::json!({
            "error": "conflict", "message": error.to_string()
        }));
    }

    let invite_message = signal(
        Event::CallInvite,
        session.account_id,
        body.callee_uid,
        &runtime.room_id,
        call_info(&call_id, kind, "ringing", None, 0, body.offer_sdp.clone(), None),
    );
    push_to_account(&handler, body.callee_uid, &invite_message).await;

    // 60s 未接听：状态机只在 ringing 时生效，接通后自动空转
    let timeout_registry = Arc::clone(&registry);
    let timeout_handler = Arc::clone(&handler);
    let timeout_call_id = call_id.clone();
    let timeout_room = runtime.room_id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(RING_TIMEOUT_SECS)).await;
        let Ok(call) = timeout_registry.expire(&timeout_call_id).await else {
            return;
        };
        let info = call_info(&call.call_id, call.kind, "ended", Some(EndReason::Missed), 0, None, None);
        let to_caller = signal(Event::CallTimeout, call.callee_uid, call.caller_uid, &timeout_room, info.clone());
        let to_callee = signal(Event::CallTimeout, call.caller_uid, call.callee_uid, &timeout_room, info);
        push_to_account(&timeout_handler, call.caller_uid, &to_caller).await;
        push_to_account(&timeout_handler, call.callee_uid, &to_callee).await;
        timeout_registry.remove(&call.call_id).await;
    });

    HttpResponse::Ok().json(serde_json::json!({
        "call_id": call_id,
        "media_mode": MediaMode::P2p.as_str(),
        "ring_timeout_secs": RING_TIMEOUT_SECS
    }))
}

////////

/// # [HANDLER] - answer
pub async fn answer(
    request: HttpRequest,
    handler: ChatHandlerData,
    registry: RegistryData,
    body: web::Json<AnswerRequest>,
) -> HttpResponse {
    let session = match authenticated_session(&request) {
        Ok(session) => session,
        Err(response) => return response,
    };

    let call = match registry.answer(&body.call_id, session.account_id).await {
        Ok(call) => call,
        Err(error) => {
            return HttpResponse::Conflict().json(serde_json::json!({
                "error": "call_state", "message": error.to_string()
            }));
        }
    };

    let info = call_info(&call.call_id, call.kind, "connected", None, 0, Some(body.answer_sdp.clone()), None);
    // 回给主叫
    let to_caller = signal(Event::CallAnswer, call.callee_uid, call.caller_uid, &call.room_id, info.clone());
    push_to_account(&handler, call.caller_uid, &to_caller).await;
    // 同步给被叫其它设备（首答抢占后收起其它响铃）；接听者自己的设备跳过
    let state_message = signal(Event::CallState, call.callee_uid, call.callee_uid, &call.room_id, info);
    push_to_account_excluding(&handler, call.callee_uid, &state_message, Some(session.device_id.as_str())).await;

    HttpResponse::Ok().json(serde_json::json!({ "call_id": call.call_id }))
}

////////

/// # [HANDLER] - ice
pub async fn ice(
    request: HttpRequest,
    handler: ChatHandlerData,
    registry: RegistryData,
    body: web::Json<IceRequest>,
) -> HttpResponse {
    let session = match authenticated_session(&request) {
        Ok(session) => session,
        Err(response) => return response,
    };
    let Some(call) = registry.get(&body.call_id).await else {
        return HttpResponse::NotFound().json(serde_json::json!({
            "error": "not_found", "message": "call not found"
        }));
    };
    if !call.is_participant(session.account_id) {
        return HttpResponse::Forbidden().json(serde_json::json!({
            "error": "forbidden", "message": "not a participant"
        }));
    }

    let target = match body.to.trim() {
        "caller" => call.caller_uid,
        _ => call.callee_uid,
    };
    let message = signal(
        Event::CallIce,
        session.account_id,
        target,
        &call.room_id,
        call_info(&call.call_id, call.kind, call.state.as_str(), None, 0, None, Some(body.candidate.clone())),
    );
    push_to_account(&handler, target, &message).await;
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

////////

/// # [HANDLER] - incoming
/// * `DESC`: `轮询兜底：返回当前账号正在响铃的通话（含 offer SDP）`
pub async fn incoming(request: HttpRequest, registry: RegistryData) -> HttpResponse {
    let session = match authenticated_session(&request) {
        Ok(session) => session,
        Err(response) => return response,
    };

    let calls = registry.incoming_for(session.account_id).await;
    let items: Vec<serde_json::Value> = calls
        .iter()
        .map(|call| {
            serde_json::json!({
                "call_id": call.call_id,
                "room_id": call.room_id,
                "caller_uid": call.caller_uid,
                "kind": call.kind.as_str(),
                "offer_sdp": call.offer_sdp,
                "created_at": call.created_at,
            })
        })
        .collect();

    HttpResponse::Ok().json(serde_json::json!({ "calls": items }))
}

////////

/// # [HANDLER] - reject
pub async fn reject(
    request: HttpRequest,
    handler: ChatHandlerData,
    registry: RegistryData,
    body: web::Json<CallIdRequest>,
) -> HttpResponse {
    finish(request, handler, registry, body, EndReason::Rejected, Event::CallReject).await
}

/// # [HANDLER] - cancel
pub async fn cancel(
    request: HttpRequest,
    handler: ChatHandlerData,
    registry: RegistryData,
    body: web::Json<CallIdRequest>,
) -> HttpResponse {
    finish(request, handler, registry, body, EndReason::Cancelled, Event::CallCancel).await
}

/// # [HANDLER] - hangup
pub async fn hangup(
    request: HttpRequest,
    handler: ChatHandlerData,
    registry: RegistryData,
    body: web::Json<CallIdRequest>,
) -> HttpResponse {
    finish(request, handler, registry, body, EndReason::Hangup, Event::CallHangup).await
}

// # [HELPER] - 统一结束流程
async fn finish(
    request: HttpRequest,
    handler: ChatHandlerData,
    registry: RegistryData,
    body: web::Json<CallIdRequest>,
    reason: EndReason,
    event: Event,
) -> HttpResponse {
    let session = match authenticated_session(&request) {
        Ok(session) => session,
        Err(response) => return response,
    };

    let call = match registry.end(&body.call_id, session.account_id, reason).await {
        Ok(call) => call,
        Err(error) => {
            return HttpResponse::Conflict().json(serde_json::json!({
                "error": "call_state", "message": error.to_string()
            }));
        }
    };

    let duration = call.duration_ms(Utc::now());
    let peer = if session.account_id == call.caller_uid { call.callee_uid } else { call.caller_uid };
    let info = call_info(&call.call_id, call.kind, "ended", Some(reason), duration, None, None);
    let message = signal(event, session.account_id, peer, &call.room_id, info);
    push_to_account(&handler, peer, &message).await;
    registry.remove(&call.call_id).await;

    HttpResponse::Ok().json(serde_json::json!({
        "call_id": call.call_id,
        "duration_ms": duration
    }))
}

//////// END
