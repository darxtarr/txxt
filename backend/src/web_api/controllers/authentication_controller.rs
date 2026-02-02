use axum::{Json, extract::State, http::StatusCode};
use crate::{app_state::SharedState, authentication::auth, login_request::LoginRequest, login_response::LoginResponse};

pub struct AuthenticationController {}

impl AuthenticationController {
    pub async fn login(
        State(state): State<SharedState>,
        Json(payload): Json<LoginRequest>,
    ) -> Result<Json<LoginResponse>, (StatusCode, String)> {
        auth::login(State(state), Json(payload))
    }
}