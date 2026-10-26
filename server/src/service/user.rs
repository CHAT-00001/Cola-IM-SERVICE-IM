// src/services/user.rs
use crate::config::Settings;
use crate::entity::user::UserInfo;
use crate::models::message::GiftInfo;
use redis::{AsyncCommands, Client};
use serde_json;
use sqlx::PgPool;
use std::sync::Arc;

pub struct AppState {
    pub db: PgPool,
    pub redis_client: Client,
    pub config: Settings,
    pub http_client: reqwest::Client,
}

// 后续核心逻辑不变，无需额外修改
/// 获取用户信息
pub async fn get_user_info(state: Arc<AppState>, uid: &i64) -> Option<UserInfo> {
    let key = format!("user:{}", uid);

    if let Ok(mut conn) = state.redis_client.get_multiplexed_async_connection().await {
        if let Ok(cached) = conn.get::<_, String>(&key).await {
            if let Ok(user) = serde_json::from_str::<UserInfo>(&cached) {
                return Some(user);
            }
        }
    }

    if let Ok(user_opt) = sqlx::query_as::<_, UserInfo>("SELECT * FROM users WHERE id = $1")
        .bind(uid)
        .fetch_optional(&state.db)
        .await
    {
        if let Some(user) = user_opt {
            if let Ok(mut conn) = state.redis_client.get_multiplexed_async_connection().await {
                let user_json = match serde_json::to_string(&user) {
                    Ok(json) => json,
                    Err(_) => return Some(user),
                };
                let _: redis::RedisResult<()> = conn.set_ex(&key, user_json, 3600).await;
            }
            return Some(user);
        }
    }

    let url = format!(
        "{}?service=User.getInfo&uid={}",
        state.config.api.base_url, uid
    );
    if let Ok(resp) = state.http_client.get(&url).send().await {
        if let Ok(user) = resp.json::<UserInfo>().await {
            let _ = sqlx::query(
                "INSERT INTO users (id,user_nickname,avatar,level,usertype,vip_type,guard_type,liangname,car_id,car_swf,car_swftime,car_words)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
                 ON CONFLICT (id) DO UPDATE
                 SET user_nickname=EXCLUDED.user_nickname,
                     avatar=EXCLUDED.avatar,
                     level=EXCLUDED.level,
                     usertype=EXCLUDED.usertype,
                     vip_type=EXCLUDED.vip_type,
                     guard_type=EXCLUDED.guard_type,
                     liangname=EXCLUDED.liangname,
                     car_id=EXCLUDED.car_id,
                     car_swf=EXCLUDED.car_swf,
                     car_swftime=EXCLUDED.car_swftime,
                     car_words=EXCLUDED.car_words"
            )
                .bind(&user.id)
                .bind(&user.user_nickname)
                .bind(&user.avatar)
                .bind(&user.level)
                .bind(&user.usertype)
                .bind(&user.vip_type)
                .bind(&user.guard_type)
                .bind(&user.liangname)
                .bind(&user.car_id)
                .bind(&user.car_swf)
                .bind(&user.car_swftime)
                .bind(&user.car_words)
                .execute(&state.db)
                .await;

            if let Ok(mut conn) = state.redis_client.get_multiplexed_async_connection().await {
                let user_json = match serde_json::to_string(&user) {
                    Ok(json) => json,
                    Err(_) => return Some(user),
                };
                let _: redis::RedisResult<()> = conn.set_ex(&key, user_json, 3600).await;
            }

            return Some(user);
        }
    }

    None
}
