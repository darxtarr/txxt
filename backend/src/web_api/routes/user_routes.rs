use std::sync::Arc;
use axum::{Router, middleware, routing::{get, post, put, delete}};
use crate::{app_state::AppState, authentication::auth::auth_middleware, user_controller::UserController};

pub const ROUTER_PATH: &str = "/user";

pub fn get_router(app_state:Arc<AppState>) -> Router {
    Router::new()
        .route(format!("{}/get", ROUTER_PATH).as_str(), get(UserController::get))
        .route(format!("{}/get_all", ROUTER_PATH).as_str(), get(UserController::get_all))
        .route(format!("{}/add", ROUTER_PATH).as_str(), post(UserController::add))
        .route(format!("{}/delete", ROUTER_PATH).as_str(), delete(UserController::delete))
        .route(format!("{}/edit", ROUTER_PATH).as_str(), put(UserController::edit))
        .layer(middleware::from_fn_with_state(app_state.clone(), auth_middleware))
        .with_state(app_state.clone())
}