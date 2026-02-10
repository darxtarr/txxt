mod auth;
mod game;
mod persist;
mod wire;
mod world;

use auth::{AppState, SharedState};
use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::services::ServeDir;

#[cfg(feature = "profile")]
fn init_tracing() {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("txxt_server=debug"));

    #[cfg(feature = "profile-console")]
    tracing_subscriber::registry()
        .with(filter)
        .with(console_subscriber::spawn())
        .with(tracing_subscriber::fmt::layer())
        .init();

    #[cfg(not(feature = "profile-console"))]
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

#[cfg(not(feature = "profile"))]
fn init_tracing() {}

#[tokio::main]
async fn main() {
    init_tracing();

    let save_path = std::env::var("TXXT_SAVE_FILE").unwrap_or_else(|_| "tasks.redb".to_string());

    // ── Boot the World ─────────────────────────────────────────
    let save_file = persist::SaveFile::open(&save_path)
        .expect("Failed to open save file");
    #[cfg(feature = "profile")]
    tracing::info!(save_path = %save_path, "opened save file");

    let mut world = save_file.load_world()
        .expect("Failed to load world from save file");

    // Seed defaults if empty
    let svc_count = save_file.ensure_default_services(&mut world)
        .expect("Failed to seed services");
    if svc_count > 0 {
        println!("Seeded {svc_count} default services");
        #[cfg(feature = "profile")]
        tracing::info!(svc_count, "seeded default services");
    }

    if save_file.ensure_default_user(&mut world)
        .expect("Failed to seed user")
    {
        println!("Created default admin user (admin / admin)");
        #[cfg(feature = "profile")]
        tracing::info!("created default admin user");
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

    // ── Resolve IRONCLAD path relative to Cargo.toml ────────────
    let ironclad_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../frontend");
    println!("  Static:  {}", ironclad_dir.display());
    #[cfg(feature = "profile")]
    tracing::info!(static_dir = %ironclad_dir.display(), "frontend directory resolved");

    // ── Router ─────────────────────────────────────────────────
    let app = Router::new()
        // Auth (REST, JSON — called once per session)
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        // Game WebSocket (binary protocol — the real data path)
        .route("/api/game", get(game::ws_handler))
        // Static files — serve IRONCLAD renderer from txxt2 repo
        .fallback_service(ServeDir::new(&ironclad_dir).append_index_html_on_directories(true))
        .with_state(state);

    // ── Start ──────────────────────────────────────────────────
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("Server running on http://localhost:3000");
    println!("  Game WS: ws://localhost:3000/api/game");
    println!("  Login:   POST http://localhost:3000/api/auth/login");
    #[cfg(feature = "profile")]
    tracing::info!("server start listening");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
