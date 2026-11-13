// server/src/call/registry.rs -- 通话注册表（内存状态机）
// 2026-11-12 16:42 Created.
//
// 单节点内存注册表：call_id → CallRuntime。
// 规则：只有被叫能接听、首答抢占、只有参与方能挂断、超时只对 ringing 生效。
// 多节点阶段再把这层换成 Redis 路由表（接口保持不变）。

////////

use std::collections::HashMap;

use chrono::Utc;
use tokio::sync::RwLock;

use super::model::{CallRuntime, CallState, EndReason};

////////

/// # [ERROR] - 通话状态机错误
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CallError {
    #[error("call not found")]
    NotFound,
    #[error("call already ended")]
    Ended,
    #[error("call already answered")]
    AlreadyAnswered,
    #[error("not a participant of this call")]
    Unauthorized,
    #[error("invalid call state: {0}")]
    InvalidState(&'static str),
}

////////

/// # [REGISTRY] - 通话注册表
#[derive(Default)]
pub struct CallRegistry {
    calls: RwLock<HashMap<String, CallRuntime>>,
}

impl CallRegistry {
    //

    ////////

    /// # 1. [REGISTRY] - 登记新通话（必须处于 ringing）
    pub async fn create(&self, call: CallRuntime) -> Result<CallRuntime, CallError> {
        if call.state != CallState::Ringing {
            return Err(CallError::InvalidState("create requires ringing"));
        }
        let mut calls = self.calls.write().await;
        if calls.contains_key(&call.call_id) {
            return Err(CallError::InvalidState("duplicate call id"));
        }
        calls.insert(call.call_id.clone(), call.clone());
        Ok(call)
    }

    ////////

    /// # 2. [REGISTRY] - 读取通话
    pub async fn get(&self, call_id: &str) -> Option<CallRuntime> {
        self.calls.read().await.get(call_id).cloned()
    }

    ////////

    /// # 3. [REGISTRY] - 接听
    /// * `DESC`: `只有被叫能接听；首答抢占，重复接听返回 AlreadyAnswered`
    pub async fn answer(&self, call_id: &str, uid: u64) -> Result<CallRuntime, CallError> {
        let mut calls = self.calls.write().await;
        let call = calls.get_mut(call_id).ok_or(CallError::NotFound)?;

        match call.state {
            CallState::Ended => return Err(CallError::Ended),
            CallState::Connected => return Err(CallError::AlreadyAnswered),
            CallState::Ringing => {}
        }
        if uid != call.callee_uid {
            return Err(CallError::Unauthorized);
        }

        call.state = CallState::Connected;
        call.answered_at = Some(Utc::now());
        Ok(call.clone())
    }

    ////////

    /// # 4. [REGISTRY] - 结束通话
    /// * `DESC`: `参与方均可结束；重复结束返回 Ended`
    pub async fn end(
        &self,
        call_id: &str,
        uid: u64,
        reason: EndReason,
    ) -> Result<CallRuntime, CallError> {
        let mut calls = self.calls.write().await;
        let call = calls.get_mut(call_id).ok_or(CallError::NotFound)?;

        if call.state == CallState::Ended {
            return Err(CallError::Ended);
        }
        if !call.is_participant(uid) {
            return Err(CallError::Unauthorized);
        }

        call.state = CallState::Ended;
        call.end_reason = Some(reason);
        Ok(call.clone())
    }

    ////////

    /// # 5. [REGISTRY] - 振铃超时
    /// * `DESC`: `只对仍在 ringing 的通话生效（接通后超时任务应被取消/空转）`
    pub async fn expire(&self, call_id: &str) -> Result<CallRuntime, CallError> {
        let mut calls = self.calls.write().await;
        let call = calls.get_mut(call_id).ok_or(CallError::NotFound)?;

        if call.state != CallState::Ringing {
            return Err(CallError::InvalidState("timeout requires ringing"));
        }

        call.state = CallState::Ended;
        call.end_reason = Some(EndReason::Missed);
        Ok(call.clone())
    }

    ////////

    /// # 6. [REGISTRY] - 被叫的来电列表（轮询兜底）
    /// * `DESC`: `WS 帧丢失/页面刚加载时，客户端轮询这里补弹来电窗`
    pub async fn incoming_for(&self, uid: u64) -> Vec<CallRuntime> {
        self.calls
            .read()
            .await
            .values()
            .filter(|call| call.callee_uid == uid && call.state == CallState::Ringing)
            .cloned()
            .collect()
    }

    ////////

    /// # 7. [REGISTRY] - 清理
    pub async fn remove(&self, call_id: &str) {
        self.calls.write().await.remove(call_id);
    }

    ////////

    /// # 8. [REGISTRY] - 当前通话数
    pub async fn len(&self) -> usize {
        self.calls.read().await.len()
    }
}

////////

#[cfg(test)]
mod tests {
    use super::{CallError, CallRegistry};
    use crate::call::model::{CallKind, CallRuntime, CallState, EndReason};
    use chrono::Utc;

    fn ringing_call(call_id: &str) -> CallRuntime {
        CallRuntime {
            call_id: call_id.to_owned(),
            room_id: "chat_1".to_owned(),
            caller_uid: 100,
            callee_uid: 200,
            caller_device: "phone".to_owned(),
            offer_sdp: None,
            kind: CallKind::Video,
            state: CallState::Ringing,
            created_at: Utc::now(),
            answered_at: None,
            end_reason: None,
        }
    }

    #[tokio::test]
    async fn callee_answers_and_first_answer_wins() {
        let registry = CallRegistry::default();
        registry.create(ringing_call("c1")).await.unwrap();

        let answered = registry.answer("c1", 200).await.unwrap();
        assert_eq!(answered.state, CallState::Connected);
        assert!(answered.answered_at.is_some());

        assert_eq!(registry.answer("c1", 200).await.unwrap_err(), CallError::AlreadyAnswered);
    }

    #[tokio::test]
    async fn caller_cannot_answer_own_call() {
        let registry = CallRegistry::default();
        registry.create(ringing_call("c2")).await.unwrap();
        assert_eq!(registry.answer("c2", 100).await.unwrap_err(), CallError::Unauthorized);
    }

    #[tokio::test]
    async fn stranger_cannot_end_call() {
        let registry = CallRegistry::default();
        registry.create(ringing_call("c3")).await.unwrap();
        assert_eq!(registry.end("c3", 999, EndReason::Hangup).await.unwrap_err(), CallError::Unauthorized);
    }

    #[tokio::test]
    async fn timeout_only_applies_while_ringing() {
        let registry = CallRegistry::default();
        registry.create(ringing_call("c4")).await.unwrap();
        registry.answer("c4", 200).await.unwrap();

        assert!(matches!(
            registry.expire("c4").await.unwrap_err(),
            CallError::InvalidState(_)
        ));
    }

    #[tokio::test]
    async fn timeout_marks_missed_and_end_is_idempotent() {
        let registry = CallRegistry::default();
        registry.create(ringing_call("c5")).await.unwrap();

        let expired = registry.expire("c5").await.unwrap();
        assert_eq!(expired.state, CallState::Ended);
        assert_eq!(expired.end_reason, Some(EndReason::Missed));

        assert_eq!(registry.end("c5", 100, EndReason::Hangup).await.unwrap_err(), CallError::Ended);
    }

    #[tokio::test]
    async fn duration_is_zero_before_answer() {
        let call = ringing_call("c6");
        assert_eq!(call.duration_ms(Utc::now()), 0);
    }
}

//////// END
