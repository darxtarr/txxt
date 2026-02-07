use axum::{Json, extract::{Query, State}, http::StatusCode};
use uuid::Uuid;

use crate::{app_state::SharedState, user::User, user_add_request::UserAddRequest, user_edit_request::UserEditRequest, user_get_response::UserGetResponse};

pub struct UserController {}

impl UserController {
    pub async fn get(
        State(state): State<SharedState>,
        Query(id): Query<Uuid>) -> Result<Json<UserGetResponse>, (StatusCode, String)> {
        match state.data_context.get_user(id) {
            Ok(Some(user)) => return Ok(Json(user.to_get_dto())),
            _ => return Err((StatusCode::NOT_FOUND, "User not found".to_string()))
        }
    }

    pub async fn get_all(State(state): State<SharedState>) -> Result<Json<Vec<UserGetResponse>>, (StatusCode, String)> {
        state.data_context.list_users()
            .map(|vec| Json(vec.into_iter().map(|u| u.to_get_dto()).collect()))
            .map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Error while getting users: {}", e.to_string()))
            })
    }

    pub async fn add(
        State(state): State<SharedState>,
        Json(body): Json<UserAddRequest>) -> Result<(), (StatusCode, String)> {
        let user = User::new(body);
        match state.data_context.create_user(&user) {
            Ok(_) => Ok(()),
            Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Error inserting user: {}", e.to_string())))
        }
    }

    pub async fn delete(
        State(state): State<SharedState>,
        Query(id): Query<Uuid>) -> Result<(), (StatusCode, String)> {
        match state.data_context.delete_user(id) {
            Ok(true) => Ok(()),
            Ok(false) => Err((StatusCode::NOT_FOUND, "User not found".to_string())),
            Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Error deleting user: {}", e.to_string())))
        }
    }

    pub async fn edit(
        State(state): State<SharedState>,
        Query(id): Query<Uuid>,
        Json(body): Json<UserEditRequest>) -> Result<(), (StatusCode, String)> {
        match state.data_context.edit_user(id, body) {
            Ok(true) => Ok(()),
            Ok(false) => Err((StatusCode::NOT_FOUND, "User to update was not found".to_string())),
            Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Error updating user: {}", e.to_string())))
        }
    }
}