use std::sync::Arc;
use axum::{Router, middleware, routing::get};
use crate::{app_state::AppState, authentication::auth::auth_middleware};
use super::super::controllers::health_controller::HealthController;

pub const ROUTER_PATH: &str = "/health";

pub fn get_router(app_state: Arc<AppState>) -> Router {
    Router::new()
        .route(format!("{}/check_status", ROUTER_PATH).as_str(), get(HealthController::get))
        .layer(middleware::from_fn_with_state(app_state.clone(), auth_middleware))
        .with_state(app_state)
}