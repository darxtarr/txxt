mod api;
mod auth;
mod db;
mod models;
mod ws;

use auth::{AppState, SharedState};
use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
};

#[tokio::main]
async fn main() {
    // Initialize database
    let db = db::Db::new("tasks.redb").expect("Failed to open database");

    // Create default admin user if needed
    db.ensure_default_user().expect("Failed to create default user");

    // Create broadcast channel for WebSocket
    let (tx, _rx) = broadcast::channel::<String>(100);

    // Create shared state
    let state: SharedState = Arc::new(AppState {
        db,
        ws_broadcast: tx,
    });

    // Build router
    let app = Router::new()
        // Auth routes (no auth required)
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        // WebSocket (auth handled differently)
        .route("/api/ws", get(ws::ws_handler))
        // Protected API routes
        .nest(
            "/api",
            Router::new()
                .route("/tasks", get(api::list_tasks).post(api::create_task))
                .route(
                    "/tasks/:id",
                    get(api::get_task)
                        .put(api::update_task)
                        .delete(api::delete_task),
                )
                .route("/users", get(api::list_users))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    auth::auth_middleware,
                )),
        )
        // Serve static files from frontend/dist
        .fallback_service(ServeDir::new("../frontend/dist").append_index_html_on_directories(true))
        // Add state
        .with_state(state)
        // CORS for development
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("Server running on http://localhost:3000");
    println!("Default login: admin / admin");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
