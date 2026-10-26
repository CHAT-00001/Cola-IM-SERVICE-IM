// crates/storage_pg/src/repository/mod.rs -- PostgreSQL repository module declarations
// 2026/10/7 21:46 Created.

////////

pub mod chat_session;
pub mod contact;
pub mod group;
pub mod message;
mod sql_error;

////////

pub(crate) use sql_error::log_sqlx_error;
