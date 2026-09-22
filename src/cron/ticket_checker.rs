use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use crate::{
    common::{AppError, AppState},
    entities::{ticket, Ticket},
};

pub const STATUS_OPEN: i32 = 0;
pub const STATUS_CLOSED: i32 = 1;
pub const REPLY_STATUS_REPLIED: i32 = 1;

/// Auto-closes tickets that have been replied to and have been inactive for over 24 hours.
/// Returns the number of tickets closed.
pub async fn check_and_close_tickets(state: &AppState) -> Result<usize, AppError> {
    let now = chrono::Utc::now().timestamp();
    let one_day_ago = now - 24 * 3600;

    let tickets = Ticket::find()
        .filter(ticket::Column::Status.eq(STATUS_OPEN))
        .filter(ticket::Column::ReplyStatus.eq(REPLY_STATUS_REPLIED))
        .filter(ticket::Column::UpdatedAt.lte(one_day_ago))
        .all(&state.db)
        .await?;

    let mut closed_count = 0;
    for t in tickets {
        let mut t_active: ticket::ActiveModel = t.into();
        t_active.status = Set(STATUS_CLOSED);
        t_active.updated_at = Set(now);
        t_active.update(&state.db).await?;
        closed_count += 1;
    }

    Ok(closed_count)
}
