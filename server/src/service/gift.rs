// src/services/gift.rs -- IM - SERVICE - 礼物服务层
// 2026-01-10 14:20

////////

use crate::config::Settings;
use crate::entity::gift::GiftInfo;
use redis::{AsyncCommands, Client};
use serde_json;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{error, info, warn};

////////

/// # [STATE] - 应用状态
pub struct AppState {
    pub db: PgPool,
    pub redis_client: Client,
    pub config: Settings,
    pub http_client: reqwest::Client,
}

////////

/// 公共辅助函数：将GiftInfo写入Redis缓存（过期时间3600秒）
async fn cache_gift_info(redis_client: &Client, key: &str, gift: &GiftInfo) {
    let Some(mut conn) = redis_client.get_multiplexed_async_connection().await.ok() else {
        warn!("Redis获取连接失败，无法写入缓存，key: {}", key);
        return;
    };

    let Some(gift_json) = serde_json::to_string(gift).ok() else {
        error!("GiftInfo序列化失败，无法写入缓存，key: {}", key);
        return;
    };

    if let Err(e) = conn.set_ex::<&str, String, ()>(key, gift_json, 3600).await {
        error!("Redis缓存写入失败，key: {}, 错误: {}", key, e);
    }
}

/// 获取礼物信息（优先缓存，其次数据库，最后远程API）
pub async fn get_gift_info(state: Arc<AppState>, gift_id: &str) -> Option<GiftInfo> {
    let key = format!("gift:{}", gift_id);

    // 1. 从Redis缓存获取
    if let Ok(mut conn) = state.redis_client.get_multiplexed_async_connection().await {
        match conn.get::<_, String>(&key).await {
            Ok(cached) => match serde_json::from_str::<GiftInfo>(&cached) {
                Ok(gift) => return Some(gift),
                Err(e) => error!("Redis缓存反序列化失败，key: {}, 错误: {}", key, e),
            },
            Err(e) => warn!("Redis缓存查询失败，key: {}, 错误: {}", key, e),
        }
    } else {
        warn!("Redis获取连接失败，key: {}", key);
    }

    // 2. 从数据库获取
    match sqlx::query_as::<_, GiftInfo>("SELECT * FROM gifts WHERE id = $1")
        .bind(gift_id)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(gift)) => {
            // 调用公共函数写入Redis缓存
            cache_gift_info(&state.redis_client, &key, &gift).await;
            return Some(gift);
        }
        Ok(None) => warn!("数据库中未找到礼物信息，gift_id: {}", gift_id),
        Err(e) => error!("数据库查询失败，gift_id: {}, 错误: {}", gift_id, e),
    }

    // 3. 从远程API获取
    let url = format!(
        "{}?service=Gift.getInfo&giftid={}",
        state.config.api.base_url, gift_id
    );
    match state.http_client.get(&url).send().await {
        Ok(resp) => {
            let status = resp.status(); // 提前获取状态码
            if !status.is_success() {
                // 优化：读取响应体辅助排查错误
                let resp_body = resp
                    .text()
                    .await
                    .unwrap_or_else(|_| "无法读取响应体".to_string());
                error!(
                    "远程API请求返回非成功状态码，url: {}, 状态码: {}, 响应体: {}",
                    url, status, resp_body
                );
                return None;
            }
            match resp.json::<GiftInfo>().await {
                Ok(gift) => {
                    // 写入数据库（处理冲突，不忽略执行结果）
                    match sqlx::query(
                        "INSERT INTO gifts (id,name,count,length,icon)
                         VALUES ($1,$2,$3,$4,$5)
                         ON CONFLICT (id) DO UPDATE
                         SET name=EXCLUDED.name,
                             count=EXCLUDED.count,
                             length=EXCLUDED.length,
                             icon=EXCLUDED.icon",
                    )
                    .bind(&gift.id)
                    .bind(&gift.name)
                    .bind(&gift.count)
                    .bind(&gift.length)
                    .bind(&gift.icon)
                    .execute(&state.db)
                    .await
                    {
                        Ok(_) => info!("数据库插入/更新礼物信息成功，gift_id: {}", gift_id),
                        Err(e) => error!(
                            "数据库插入/更新礼物信息失败，gift_id: {}, 错误: {}",
                            gift_id, e
                        ),
                    }

                    // 调用公共函数写入Redis缓存
                    cache_gift_info(&state.redis_client, &key, &gift).await;

                    return Some(gift);
                }
                Err(e) => error!("远程API响应反序列化失败，url: {}, 错误: {}", url, e),
            }
        }
        Err(e) => error!("远程API请求发送失败，url: {}, 错误: {}", url, e),
    }

    None
}
