mod auth;
mod game;
mod persist;
mod world;

use auth::{AppState, SharedState};
use axum::{
    routing::{get, post},
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
    // ── Boot the World ─────────────────────────────────────────
    let save_file = persist::SaveFile::open("tasks.redb")
        .expect("Failed to open save file");

    let mut world = save_file.load_world()
        .expect("Failed to load world from save file");

    // Seed defaults if empty
    let svc_count = save_file.ensure_default_services(&mut world)
        .expect("Failed to seed services");
    if svc_count > 0 {
        println!("Seeded {svc_count} default services");
    }

    if save_file.ensure_default_user(&mut world)
        .expect("Failed to seed user")
    {
        println!("Created default admin user (admin / admin)");
    }

    println!(
        "World loaded: {} tasks, {} users, {} services, revision {}",
        world.tasks.len(),
        world.users.len(),
        world.services.len(),
        world.revision,
    );

    // ── Broadcast channel ──────────────────────────────────────
    let (game_tx, _) = broadcast::channel::<Vec<u8>>(256);

    // ── Shared state ───────────────────────────────────────────
    let state: SharedState = Arc::new(AppState {
        world: std::sync::RwLock::new(world),
        save_file,
        game_tx,
    });

    // ── Router ─────────────────────────────────────────────────
    let app = Router::new()
        // Auth (REST, JSON — called once per session)
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        // Game WebSocket (binary protocol — the real data path)
        .route("/api/game", get(game::ws_handler))
        // Static files
        .fallback_service(ServeDir::new("../frontend/dist").append_index_html_on_directories(true))
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    // ── Start ──────────────────────────────────────────────────
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("Server running on http://localhost:3000");
    println!("  Game WS: ws://localhost:3000/api/game");
    println!("  Login:   POST http://localhost:3000/api/auth/login");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
