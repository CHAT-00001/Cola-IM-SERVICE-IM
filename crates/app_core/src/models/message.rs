// app_core/src/models/message.rs  -- 消息模型
// 2026-03-24 04:19

////////

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

////////

/// # [QUERY] - 消息查询
pub struct MessageQuery {
    pub room_id: String,              // 房号
    pub last_msg_time: DateTime<Utc>, // 最后消息时间
    pub limit: i32,                   // 数量
}

////////

/// # [BODY] - 消息体
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub id: String,
    pub status: Status, // 状态码： 0. 草稿  1. 正在发送  2. 同步到收件箱 3. 同步到收件箱  4. 已读
    pub client_id: String, // 客户端id
    pub send_at: DateTime<Utc>, // 发送时间
    pub server_sync_at: Option<DateTime<Utc>>, // 服务端同步时间
    pub send_id: String, // 消息id（客户端创建）
    pub sync_id: String, // 同步id（服务端创建）
    pub msg_type: MsgType, // 类型：0.系统 1. 文字 2. 音频 3. 视频 4. 照片 5. 文件 6. 红包 7. 弹幕 8.
    pub event: Event,      // 事件
    pub room_id: String,   // 房间 ID
    pub uid: u64,          // 发送者 uid
    pub sender: Option<UserInfo>, // 发送者资料
    pub payload: Payload,  // 消息负载
    pub channel: Option<MsgChannel>, // 消息管道
}

////////

/// # [STATUS] - Message Status
#[repr(u32)]
#[derive(Debug, Serialize_repr, Deserialize_repr, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Editing = 0,  // 系统
    Sending = 1,  // 文字
    Syncing = 2,  // 同步发件箱/收件箱
    received = 3, // 客户端接收
    read = 4,     // 已读
    recalled = 5, // 撤回
}

/// ## Message Type
#[repr(u32)]
#[derive(Debug, Serialize_repr, Deserialize_repr, Clone, Copy, PartialEq, Eq)] // 这里必须用 _repr 处理数字
pub enum MsgType {
    System = 0,     // 系统
    Text = 1,       // 文字
    Audio = 2,      // 音频
    Video = 3,      // 视频
    Image = 4,      // 图像
    File = 5,       // 文件
    RedPacket = 6,  // 红包
    Barrage = 7,    // 弹幕
    Location = 8,   // 位置
    Card = 9,       // 名片
    LivePhoto = 10, // 实况照片
    Emoticon = 11,  // 表情包
    VideoCall = 12, // 视频通话
    Transfer = 13,  // 转账
    Gift = 14,      // 礼物
}

/// ## Event
#[repr(u32)]
#[derive(Debug, Serialize_repr, Deserialize_repr, Clone, Copy, PartialEq, Eq)] // 这里必须用 _repr 处理数字
pub enum Event {
    SendMsg = 0,          // 发送消息
    Connect = 1,          // 建立连接
    ReConn = 2,           // 重连中
    Coming = 3,           // 来到直播间
    KillUser = 4,         // 踢人
    ChanLiveType = 5,     // 改变直播类型
    Follow = 6,           // 关注
    VideoCallWaiting = 7, // 等待
    VideoCallStart = 8,   // 接通
    VideoCallPause = 9,   // 暂停
    VideoCallEnd = 10,    // 挂断
}

/// ## Payload
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "content")]
pub enum Payload {
    #[serde(rename = "System")]
    System { message: String }, // 系统消息

    #[serde(rename = "Content")]
    Content { text: String }, // 内容

    #[serde(rename = "Media")]
    Media(MediaInfo), // 媒体

    #[serde(rename = "Gift")]
    Gift(GiftInfo), // 礼物

    #[serde(rename = "Barrage")]
    Barrage(BarrageInfo), // 弹幕
}

/// ## Media Info
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct MediaInfo {
    pub name: Option<String>,     // 名称
    pub size: Option<u64>,        // 尺寸
    pub url: Option<String>,      // 媒体位置
    pub width: Option<u64>,       // 宽度
    pub height: Option<u64>,      // 高度
    pub fps: Option<f32>,         // 帧数
    pub bit: Option<u16>,         // 色深bit
    pub link_url: Option<String>, // 链接URL
    pub is_livephoto: Option<u8>, // 是否实况照片
}

/// ## 用户信息
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserInfo {
    pub id: u64,
    pub user_nickname: String,
    pub avatar: String,
    pub level: String,
    pub usertype: String,
    pub vip_type: String,
    pub guard_type: String,
    pub liangname: String,
    pub car_id: String,
    pub car_swf: String,
    pub car_swftime: String,
    pub car_words: String,
}

/// 消息内容
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Content {
    pub text: Option<String>,
}

/// 礼物信息
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GiftInfo {
    pub id: String,   // ID
    pub name: String, // 名称
    pub count: u32,   // 数量，默认1
    pub length: u32,  // 礼物动画时长
    pub icon: String, // 图标
}

/// ## 弹幕内容
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BarrageInfo {
    pub id: String,
    pub uid: u64,
    pub room_id: String,
    pub text: String,
    pub color: String,
}

/// ## Message Chanel 消息管道
#[repr(u32)]
#[derive(Debug, Serialize_repr, Deserialize_repr, Clone, Copy, PartialEq, Eq)]
pub enum MsgChannel {
    public = 0,   // 公共聊天
    single = 1,   // 单人聊天
    group = 2,    // 群聊
    received = 3, // 战斗信息
    Notice = 4,   // 通知
}

//////// END