pub mod health_routes;

use axum::Router;

pub fn map_routes() -> Router {
    Router::new()
        .merge(health_routes::get_router())
}