// crates/storage_pg/src/repository/sql_error.rs -- Shared sqlx error logging for PostgreSQL repositories
// 2026/10/11 20:30 Created.

////////

/// # [HANDLER] - log_sqlx_error
/// * `DESC`: `打印 sqlx 执行错误，附带 PostgreSQL 错误码、约束、表名与明细，便于排查`
pub(crate) fn log_sqlx_error(operation: &str, error: &sqlx::Error) {
    match error.as_database_error() {
        Some(db_error) => {
            let pg_error = db_error
                .as_error()
                .downcast_ref::<sqlx::postgres::PgDatabaseError>();
            let detail = pg_error.and_then(|pg| pg.detail()).unwrap_or("-");
            let hint = pg_error.and_then(|pg| pg.hint()).unwrap_or("-");
            tracing::error!(
                target: "storage_pg::sql",
                operation = operation,
                sqlstate = db_error.code().as_deref().unwrap_or("-"),
                constraint = db_error.constraint().unwrap_or("-"),
                table = db_error.table().unwrap_or("-"),
                detail = detail,
                hint = hint,
                db_message = db_error.message(),
                "❌ [PG] SQL 执行失败"
            )
        }
        None => tracing::error!(
            target: "storage_pg::sql",
            operation = operation,
            error = %error,
            "❌ [PG] SQL 执行失败（非数据库错误）"
        ),
    }
}

////////

#[cfg(test)]
mod tests {
    use super::log_sqlx_error;

    /// # [HANDLER] - logs_non_database_error_without_panicking
    /// * `DESC`: `验证非数据库类 sqlx 错误也能被安全打印而不 panic`
    #[test]
    fn logs_non_database_error_without_panicking() {
        log_sqlx_error("test.operation", &sqlx::Error::RowNotFound);
    }
}

//////// END