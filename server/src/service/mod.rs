// server/src/service/mod -- 服务器 - service - mod
// 2026-10-11 17:21

////////

pub mod blocklist; // 黑名单
pub mod contact_sync; // 联系人模型同步（单聊准入）
pub mod gift; // 礼物
pub mod identity; // 用户资料跨进程调用（HTTP / gRPC）
pub mod mongodb; // 芒果
pub mod relationships; // 关系船
pub mod state; // 状态机
pub mod user; // 用户
