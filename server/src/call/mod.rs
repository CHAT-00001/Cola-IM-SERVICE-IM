// server/src/call/mod.rs -- 通话模块（端到端 WebRTC 信令 + 预留中央推流）
// 2026-11-12 16:45 Created.

////////

pub mod media;
pub mod model;
pub mod registry;

pub use media::{CentralMediaPlane, MediaMode, MediaPlane, MediaTicket, P2pMediaPlane};
pub use model::{CallKind, CallRuntime, CallState, EndReason};
pub use registry::{CallError, CallRegistry};

//////// END
