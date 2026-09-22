pub mod commission_checker;
pub mod log_cleaner;
pub mod node_checker;
pub mod order_checker;
pub mod stat_recorder;
pub mod ticket_checker;
pub mod traffic_resetter;

pub use commission_checker::check_and_pay_commissions;
pub use log_cleaner::clean_logs;
pub use node_checker::check_and_cleanup_nodes;
pub use order_checker::check_orders;
pub use stat_recorder::record_daily_stats;
pub use ticket_checker::check_and_close_tickets;
pub use traffic_resetter::{calculate_next_reset_time, check_and_reset_traffic};

use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::{error, info};

use crate::common::AppState;

/// Spawns the background cron scheduler using Tokio tasks.
/// Ticks every 60 seconds and coordinates minute-level, 5-minute, and daily jobs.
pub fn start_cron_scheduler(state: AppState) -> JoinHandle<()> {
    tokio::spawn(async move {
        info!("Starting Xboard-RS cron scheduler loop");
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        let mut minute_counter: u64 = 0;

        loop {
            interval.tick().await;
            minute_counter = minute_counter.wrapping_add(1);

            // 1. Every minute: Orders, Traffic Reset, Commissions, Tickets
            if let Err(e) = order_checker::check_orders(&state).await {
                error!("Cron check_orders error: {:?}", e);
            }

            if let Err(e) = traffic_resetter::check_and_reset_traffic(&state).await {
                error!("Cron check_and_reset_traffic error: {:?}", e);
            }

            if let Err(e) = commission_checker::check_and_pay_commissions(&state).await {
                error!("Cron check_and_pay_commissions error: {:?}", e);
            }

            if let Err(e) = ticket_checker::check_and_close_tickets(&state).await {
                error!("Cron check_and_close_tickets error: {:?}", e);
            }

            // 2. Every 5 minutes: Cleanup node/device state
            if minute_counter.is_multiple_of(5) {
                if let Err(e) = node_checker::check_and_cleanup_nodes(&state).await {
                    error!("Cron check_and_cleanup_nodes error: {:?}", e);
                }
            }

            // 3. Daily (every 1440 minutes): Daily stats recording & log cleanup
            if minute_counter.is_multiple_of(1440) {
                if let Err(e) = stat_recorder::record_daily_stats(&state).await {
                    error!("Cron record_daily_stats error: {:?}", e);
                }
                if let Err(e) = log_cleaner::clean_logs(&state).await {
                    error!("Cron clean_logs error: {:?}", e);
                }
            }
        }
    })
}
