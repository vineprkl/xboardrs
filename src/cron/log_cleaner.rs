use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::{
    common::{AppError, AppState},
    entities::{
        admin_audit_log,
        stat::{server as stat_server, user as stat_user},
        AdminAuditLog, StatServer, StatUser,
    },
};

/// Cleans expired logs matching PHP `ResetLog`.
/// - Deletes user and server traffic statistics older than 2 months (60 days).
/// - Deletes admin audit logs older than 3 months (90 days).
///
/// Returns tuple of (deleted_stat_user, deleted_stat_server, deleted_audit_logs).
pub async fn clean_logs(state: &AppState) -> Result<(u64, u64, u64), AppError> {
    let now = chrono::Utc::now().timestamp();
    let two_months_ago = now - 60 * 86400;
    let three_months_ago = now - 90 * 86400;

    let res_users = StatUser::delete_many()
        .filter(stat_user::Column::RecordAt.lt(two_months_ago))
        .exec(&state.db)
        .await?;

    let res_servers = StatServer::delete_many()
        .filter(stat_server::Column::RecordAt.lt(two_months_ago))
        .exec(&state.db)
        .await?;

    let res_audits = AdminAuditLog::delete_many()
        .filter(admin_audit_log::Column::CreatedAt.lt(three_months_ago))
        .exec(&state.db)
        .await?;

    Ok((
        res_users.rows_affected,
        res_servers.rows_affected,
        res_audits.rows_affected,
    ))
}
