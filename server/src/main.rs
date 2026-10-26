// server/src/main.rs  -- 主程序模块入口
// 2026-01-04 09:40

////////

mod app;
mod chat;
mod codec;
mod config;
mod entity;
mod handlers;
mod hub;
mod message_handlers;
mod middleware;
mod models;
mod service;
mod session_handlers;
mod shared_handler;
mod ticket_handlers;

////////

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // 🚧 - MAIN - 主程序编排

    ////////

    // 💡 - 加载本地 .env（不覆盖已存在的真实环境变量，便于注入 COLA_JWT_SECRET 等）
    let _ = dotenvy::dotenv();

    ////////

    // 💡 - 设置日志记录
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,mongodb=debug"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    ////////

    // 💡 - LOGGER - 日志
    println!("Hello, world!");

    ////////

    // 💡 - RUN - 运行
    app::run().await
}

//////// END
