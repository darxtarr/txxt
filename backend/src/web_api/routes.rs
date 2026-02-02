pub mod authentication_routes;
pub mod health_routes;
pub mod user_routes;

use std::sync::Arc;
use axum::Router;
use crate::app_state::AppState;

const API_PATH: &str = "/api";

pub fn map_routes(app_state: Arc<AppState>) -> Router {
    Router::new()
        .nest(format!("{}", API_PATH).as_str(), health_routes::get_router(app_state.clone()))
        .nest(format!("{}", API_PATH).as_str(), authentication_routes::get_router(app_state.clone()))
        .nest(format!("{}", API_PATH).as_str(), user_routes::get_router(app_state.clone()))
}
