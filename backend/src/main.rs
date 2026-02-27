mod game;
mod persist;
mod renderer;
mod spatial;
mod wire;
mod world;

use game::{AppState, SharedState};
use persist::SaveFile;
use renderer::{Renderer, ViewState};
use spatial::SdfOverlay;
use wire::ImageFormat;

use axum::Router;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() {
    let save_path = std::env::var("TXXT_DB").unwrap_or_else(|_| "txxt.redb".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    // Boot: open save file, load world, seed defaults
    let save_file = SaveFile::open(&save_path).expect("failed to open save file");
    let mut world = save_file.load_world().expect("failed to load world");

    let svc_count = save_file.ensure_default_services(&mut world).expect("failed to seed services");
    let user_created = save_file.ensure_default_user(&mut world).expect("failed to seed user");

    if svc_count > 0 {
        println!("[boot] seeded {svc_count} default services");
    }
    if user_created {
        println!("[boot] seeded default user");
    }

    println!(
        "[boot] loaded {} tasks, {} users, {} services, rev {}",
        world.tasks.len(),
        world.users.len(),
        world.services.len(),
        world.revision,
    );

    // Compute initial view window (current week)
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let now_minutes = (now_secs / 60) as u32;
    let now_day = now_minutes / 1440;
    let dow = (now_day + 3) % 7; // 0=Mon..6=Sun
    let window_start = (now_day - dow) * 1440;
    let window_end = window_start + 7 * 1440;

    let (game_tx, _) = broadcast::channel::<Vec<u8>>(64);

    let state: SharedState = Arc::new(AppState {
        world: RwLock::new(world),
        save_file,
        game_tx,
        renderer: Mutex::new(Renderer::new(1316, 632)),
        view: RwLock::new(ViewState { window_start, window_end }),
        shapes: RwLock::new(Vec::new()),
        overlay_gen: Arc::new(SdfOverlay),
        image_format: ImageFormat::Png,
    });

    // Router: WS at /ws, static files from frontend/
    let frontend_path = std::env::var("TXXT_FRONTEND")
        .unwrap_or_else(|_| "../frontend".to_string());

    let app = Router::new()
        .route("/ws", axum::routing::get(game::ws_handler))
        .fallback_service(ServeDir::new(frontend_path))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .expect("failed to bind");

    println!("[boot] listening on :{port}");

    axum::serve(listener, app).await.unwrap();
}
