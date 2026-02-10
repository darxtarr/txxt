use crate::persist::SaveFile;
use crate::world::World;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

// ── Auth request/response types (used to live in models.rs) ────

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserResponse,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub username: String,
}

// ── Shared state ───────────────────────────────────────────────

pub struct AppState {
    pub world: std::sync::RwLock<World>,
    pub save_file: SaveFile,
    pub game_tx: tokio::sync::broadcast::Sender<Vec<u8>>,
}

pub type SharedState = Arc<AppState>;

fn verify_password(password: &str, hash: &str) -> bool {
    let parsed_hash = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };

    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

// ── Handlers ───────────────────────────────────────────────────

pub async fn login(
    State(state): State<SharedState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    let world = state.world.read().unwrap();

    let user = world.get_user_by_username(&payload.username)
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()))?;

    if !verify_password(&payload.password, &user.password_hash) {
        return Err((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()));
    }

    let token = Uuid::new_v4().to_string();

    Ok(Json(LoginResponse {
        token,
        user: UserResponse {
            id: user.id,
            username: user.username.clone(),
        },
    }))
}

pub async fn logout() -> impl IntoResponse {
    StatusCode::OK
}
