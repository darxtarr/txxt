
use crate::{app_state::SharedState, login_request::LoginRequest, login_response::LoginResponse, settings::Settings, user::User};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    body::Body,
    extract::State,
    http::{header, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub username: String,
    pub exp: usize,
    pub iat: usize,
}



pub fn create_token(user: &User) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let settings = Settings::load().unwrap();
    let expiry = now + Duration::hours(settings.jwt_expiration_in_minutes as i64);

    let claims = Claims {
        sub: user.id,
        username: user.username.clone(),
        exp: expiry.timestamp() as usize,
        iat: now.timestamp() as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(settings.jwt_secret.as_bytes()),
    )
}

pub fn verify_token(token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let settings = Settings::load().unwrap();
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(settings.jwt_secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let parsed_hash = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };

    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

pub fn login(
    State(state): State<SharedState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    let user = state
        .data_context
        .get_user_by_username(&payload.username)
        .map_err(|e: redb::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()))?;

    if !verify_password(&payload.password, &user.password_hash) {
        return Err((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()));
    }

    let token = create_token(&user)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    println!("{} has logged in", user.username);
    Ok(Json(LoginResponse {
        token
    }))
}

pub fn logout() -> impl IntoResponse {
    StatusCode::OK
}

pub async fn auth_middleware(
    State(state): State<SharedState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    let token = match auth_header {
        Some(h) if h.starts_with("Bearer ") => Some(&h[7..]),
        _ => return Err((StatusCode::UNAUTHORIZED, "Missing or invalid token".to_string())),
    };

    let token = token.unwrap();
    let claims = verify_token(token)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid token".to_string()))?;

    let user = state
        .data_context
        .get_user(claims.sub)
        .map_err(|e: redb::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "User not found".to_string()))?;

    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}