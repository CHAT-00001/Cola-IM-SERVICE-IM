// crates/app_core/src/usecase/message.rs
// 2026-03-24 14:02

use crate::models::message::{Message, Payload};
use crate::ports::message::MessageRepository;
use chrono::Utc;

pub struct MessageUseCase<R: MessageRepository> {
    repo: R,
}

impl<R: MessageRepository> MessageUseCase<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    /// 保存消息前的校验
    pub async fn save_message(&self, msg: &Message) -> Result<String, String> {
        // 校验消息内容
        match &msg.payload {
            Payload::Content { text } => {
                if text.trim().is_empty() {
                    return Err("消息内容为空".to_string());
                }
            }
            Payload::System { message } => {
                if message.trim().is_empty() {
                    return Err("系统消息为空".to_string());
                }
            }
            Payload::Barrage(b) => {
                if b.text.trim().is_empty() {
                    return Err("弹幕内容为空".to_string());
                }
            }
            Payload::Gift(g) => {
                if g.name.trim().is_empty() {
                    return Err("礼物名称为空".to_string());
                }
            }
            Payload::Media(m) => {
                if m.url.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true) {
                    return Err("媒体 URL 为空".to_string());
                }
            }
        }

        // 设置服务端同步时间
        let mut msg_to_save = msg.clone();
        msg_to_save.server_sync_at = Some(Utc::now());

        // 保存消息
        let id = self.repo.save_message(&msg_to_save).await?;
        Ok(id)
    }
}
