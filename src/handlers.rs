// src/handlers.rs -- (备用) WebSocket 处理器模块
// 2026-01-05 11:20:00

use actix::{Actor, ActorContext, AsyncContext, Handler, Message as ActixMessage, StreamHandler};
use actix_web::{web, HttpRequest, HttpResponse, Error};
use actix_web_actors::ws;
use dashmap::DashMap;
use tracing::{error, warn, info};
use std::sync::Arc;
use std::collections::HashSet; // 引入 HashSet 处理多房间
use chrono::Utc;
use uuid::Uuid;

use crate::models::message::{MessageBody, Payload, MsgChannel, Status};
use crate::service::state::AppState;

pub type RoomChannels = Arc<DashMap<String, Vec<actix::Addr<ChatSession>>>>;

pub struct ChatSession {
    pub id: String,                 // 客户端连接 ID
    active_rooms: HashSet<String>,  // 修改：存储当前用户加入的所有房间 ID
    app_state: Arc<AppState>,
    room_channels: RoomChannels,
    self_addr: Option<actix::Addr<Self>>,
}

impl ChatSession {
    /// 辅助：获取当前 UTC 时间字符串
    fn now_utc() -> String {
        Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
    }

    /// 🚀 核心分发器：根据 channel 路由到不同的处理函数
    fn route_channel(&mut self, mut msg: MessageBody, ctx: &mut ws::WebsocketContext<Self>) {
        // 记录服务端同步时间
        msg.server_sync_at = Some(Utc::now());

        // 如果消息未带 channel，默认视为公共聊天
        let chan = msg.channel.unwrap_or(MsgChannel::public);

        match chan {
            MsgChannel::public   => self.channel_public(msg, ctx),
            MsgChannel::single   => self.channel_single(msg, ctx),
            MsgChannel::group    => self.channel_group(msg, ctx),
            MsgChannel::Notice   => self.channel_notice(msg, ctx),
            MsgChannel::received => self.channel_received(msg, ctx),
        }
    }

    /// 🌐 0. 公共聊天管道 (直播间广播)
    fn channel_public(&mut self, msg: MessageBody, ctx: &mut ws::WebsocketContext<Self>) {
        let room_id = msg.room_id.clone();
        if room_id.is_empty() { return; }

        // 实现多房间在线逻辑：如果不在已激活列表中，尝试加入
        if !self.active_rooms.contains(&room_id) {
            // 限制：最多加入 20 个房间
            if self.active_rooms.len() >= 20 {
                warn!("[{}] Client {} exceeded max rooms (20)", Self::now_utc(), self.id);
                return;
            }

            let addr = ctx.address();
            self.join_room(&room_id, addr);
            self.active_rooms.insert(room_id.clone());
            info!("[{}] [JOIN] Client {} joined Room {}", Self::now_utc(), self.id, room_id);
        }

        // 广播给房间内的所有人
        self.broadcast_to_room(&room_id, &msg);
        info!("[{}] [PUBLIC] Client {} sent msg to Room {}", Self::now_utc(), self.id, room_id);
    }

    /// 👤 1. 单人聊天管道 (1v1)
    fn channel_single(&mut self, msg: MessageBody, _ctx: &mut ws::WebsocketContext<Self>) {
        // TODO: 从全局在线用户 Map 中根据 uid 找到目标 Addr 并 do_send
        info!("[{}] [SINGLE] Client {} target UID: {}", Self::now_utc(), self.id, msg.uid);
    }

    /// 👥 2. 群聊管道
    fn channel_group(&mut self, msg: MessageBody, _ctx: &mut ws::WebsocketContext<Self>) {
        // TODO: 根据 group_id 逻辑处理
        info!("[{}] [GROUP] Client {} group ID: {}", Self::now_utc(), self.id, msg.room_id);
    }

    /// ⚔️ 3. 战斗/即时反馈信息
    fn channel_received(&mut self, _msg: MessageBody, _ctx: &mut ws::WebsocketContext<Self>) {
        // 处理战斗同步等高频信息
    }

    /// 🔔 4. 通知管道
    fn channel_notice(&mut self, msg: MessageBody, _ctx: &mut ws::WebsocketContext<Self>) {
        // 这里未来可以接入 APNs/FCM 推送
        info!("[{}] [NOTICE] System notice for Client {}", Self::now_utc(), self.id);
    }

    // 加入房间
    fn join_room(&self, room_id: &str, self_addr: actix::Addr<Self>) {
        if room_id.is_empty() { return; }
        self.room_channels
            .entry(room_id.to_string())
            .or_insert_with(Vec::new)
            .push(self_addr);
    }

    // 离开房间 (修改为从特定房间离开)
    fn leave_room(&self, room_id: &str, self_addr: &actix::Addr<Self>) {
        if room_id.is_empty() { return; }
        if let Some(mut sessions) = self.room_channels.get_mut(room_id) {
            sessions.retain(|addr| addr != self_addr);
        }
    }

    // 📢广播到房间
    fn broadcast_to_room(&self, room_id: &str, message: &MessageBody) {
        let raw_msg = match serde_json::to_string(message) {
            Ok(s) => s,
            Err(e) => { error!("Serialize error: {}", e); return; }
        };

        if let Some(room_sessions) = self.room_channels.get(room_id) {
            for session_addr in room_sessions.iter() {
                let _ = session_addr.do_send(NotifyMessage(raw_msg.clone()));
            }
        }
    }
}

// --- Actor 接口实现 ---

impl Actor for ChatSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.self_addr = Some(ctx.address());
        info!("[{}] [CONNECTED] ClientID: {} | InitialRoom: {}",
            Self::now_utc(), self.id, if self.active_rooms.is_empty() { "None".to_string() } else { self.active_rooms.iter().next().unwrap().clone() });

        // 处理初始房间
        let initial_rooms: Vec<String> = self.active_rooms.iter().cloned().collect();
        for room_id in initial_rooms {
            self.join_room(&room_id, ctx.address());
        }
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        info!("[{}] [DISCONNECTED] ClientID: {}", Self::now_utc(), self.id);
        let addr = ctx.address();
        // 彻底清理：离开所有已加入的房间
        for room_id in &self.active_rooms {
            self.leave_room(room_id, &addr);
        }
    }
}

#[derive(Clone, ActixMessage)]
#[rtype(result = "()")]
pub struct NotifyMessage(pub String);

impl Handler<NotifyMessage> for ChatSession {
    type Result = ();
    fn handle(&mut self, msg: NotifyMessage, ctx: &mut Self::Context) {
        ctx.text(msg.0);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ChatSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(text)) => {
                match serde_json::from_str::<MessageBody>(&text) {
                    Ok(parsed_msg) => self.route_channel(parsed_msg, ctx),
                    Err(e) => warn!("[{}] [JSON_ERR] {} | Data: {}", Self::now_utc(), e, text),
                }
            }
            Ok(ws::Message::Ping(p)) => ctx.pong(&p),
            Ok(ws::Message::Close(reason)) => { ctx.close(reason); ctx.stop(); }
            Err(_) => ctx.stop(),
            _ => {}
        }
    }
}

pub async fn ws_index(
    req: HttpRequest,
    stream: web::Payload,
    app_state: web::Data<Arc<AppState>>,
    room_channels: web::Data<RoomChannels>,
) -> Result<HttpResponse, Error> {
    let query: std::collections::HashMap<String, String> =
        serde_urlencoded::from_str(req.query_string()).unwrap_or_default();

    let init_room_id = query.get("room_id").cloned().unwrap_or_default();
    let client_id = Uuid::new_v4().to_string()[..8].to_string();

    let mut active_rooms = HashSet::new();
    if !init_room_id.is_empty() {
        active_rooms.insert(init_room_id);
    }

    let chat_session = ChatSession {
        id: client_id,
        active_rooms, // 使用 HashSet 初始化
        app_state: Arc::clone(app_state.get_ref()),
        room_channels: Arc::clone(room_channels.get_ref()),
        self_addr: None,
    };

    ws::start(chat_session, &req, stream)
}