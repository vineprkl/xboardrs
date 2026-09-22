pub mod group;
pub mod machine;
pub mod manage;
pub mod route;

use crate::common::AppState;
use axum::routing::{get, post};
use axum::Router;

pub fn router() -> Router<AppState> {
    Router::new()
        // Group
        .route("/group/fetch", get(group::fetch))
        .route("/group/save", post(group::save))
        .route("/group/drop", post(group::drop))
        // Route
        .route("/route/fetch", get(route::fetch))
        .route("/route/save", post(route::save))
        .route("/route/drop", post(route::drop))
        // Manage
        .route("/manage/getNodes", get(manage::get_nodes))
        .route("/manage/save", post(manage::save))
        .route("/manage/update", post(manage::update))
        .route("/manage/drop", post(manage::drop))
        .route("/manage/copy", post(manage::copy))
        .route("/manage/sort", post(manage::sort))
        .route("/manage/batchDelete", post(manage::batch_delete))
        .route("/manage/resetTraffic", post(manage::reset_traffic))
        .route(
            "/manage/batchResetTraffic",
            post(manage::batch_reset_traffic),
        )
        .route("/manage/generateEchKey", get(manage::generate_ech_key))
        // Machine
        .route("/machine/fetch", get(machine::fetch))
        .route("/machine/save", post(machine::save))
        .route("/machine/drop", post(machine::drop))
        .route("/machine/resetToken", post(machine::reset_token))
        .route("/machine/getToken", get(machine::get_token))
        .route("/machine/installCommand", get(machine::install_command))
        .route("/machine/nodes", get(machine::nodes))
        .route("/machine/history", get(machine::history))
}
