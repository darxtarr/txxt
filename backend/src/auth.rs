use crate::persist::SaveFile;
use crate::world::World;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

// JWT secret - in production, load from environment
const JWT_SECRET: &[u8] = b"your-secret-key-change-in-production";
const JWT_EXPIRY_HOURS: i64 = 24;

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

// ── JWT ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,        // user id
    pub username: String,
    pub exp: usize,       // expiry timestamp
    pub iat: usize,       // issued at
}

// ── Shared state ───────────────────────────────────────────────

pub struct AppState {
    pub world: std::sync::RwLock<World>,
    pub save_file: SaveFile,
    pub game_tx: tokio::sync::broadcast::Sender<Vec<u8>>,
}

pub type SharedState = Arc<AppState>;

// ── Helpers ────────────────────────────────────────────────────

pub fn create_token(user_id: Uuid, username: &str) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let expiry = now + Duration::hours(JWT_EXPIRY_HOURS);

    let claims = Claims {
        sub: user_id,
        username: username.to_string(),
        exp: expiry.timestamp() as usize,
        iat: now.timestamp() as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(JWT_SECRET),
    )
}

pub fn verify_token(token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(JWT_SECRET),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}

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

    let token = create_token(user.id, &user.username)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
