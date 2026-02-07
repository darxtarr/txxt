use std::sync::Arc;
use axum::{Router, routing::{post}};
use crate::{app_state::AppState, authentication_controller::AuthenticationController};

pub const ROUTER_PATH: &str = "/authentication";

pub fn get_router(app_state:Arc<AppState>) -> Router {
    Router::new()
        .route(format!("{}/login", ROUTER_PATH).as_str(), post(AuthenticationController::login)).with_state(app_state)
}