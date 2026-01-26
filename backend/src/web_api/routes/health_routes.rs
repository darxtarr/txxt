use axum::{Router, routing::{get}};
use super::super::controllers::health_controller::HealthController;

pub const ROUTER_PATH: &str = "/health";

pub fn get_router() -> Router {
    Router::new()
        .route(format!("{}/check_status", ROUTER_PATH).as_str(), get(HealthController::get))
}