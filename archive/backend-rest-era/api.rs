use crate::auth::SharedState;
use crate::models::{
    CreateTaskRequest, ServiceResponse, Task, TaskResponse, UpdateTaskRequest, User, UserResponse,
    WsMessage,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;

// Helper to convert Task to TaskResponse, resolving assigned user name from a cache
fn task_to_response(task: Task, users: &HashMap<Uuid, String>) -> TaskResponse {
    let assigned_to_name = task.assigned_to.and_then(|id| users.get(&id).cloned());

    TaskResponse {
        id: task.id,
        title: task.title,
        description: task.description,
        status: task.status,
        priority: task.priority,
        category: task.category,
        service_id: task.service_id,
        tags: task.tags,
        due_date: task.due_date,
        created_by: task.created_by,
        assigned_to: task.assigned_to,
        assigned_to_name,
        created_at: task.created_at,
        updated_at: task.updated_at,
    }
}

// Load all users into a id -> username map (avoids N+1 queries)
fn load_user_names(state: &SharedState) -> Result<HashMap<Uuid, String>, (StatusCode, String)> {
    let users = state
        .db
        .list_users()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(users.into_iter().map(|u| (u.id, u.username)).collect())
}

// Broadcast a message to all WebSocket clients
fn broadcast(state: &SharedState, msg: WsMessage) {
    if let Ok(json) = serde_json::to_string(&msg) {
        let _ = state.ws_broadcast.send(json);
    }
}

// GET /api/tasks
pub async fn list_tasks(
    State(state): State<SharedState>,
) -> Result<Json<Vec<TaskResponse>>, (StatusCode, String)> {
    let tasks = state
        .db
        .list_tasks()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let users = load_user_names(&state)?;
    let responses: Vec<TaskResponse> = tasks
        .into_iter()
        .map(|t| task_to_response(t, &users))
        .collect();

    Ok(Json(responses))
}

// POST /api/tasks
pub async fn create_task(
    State(state): State<SharedState>,
    Extension(user): Extension<User>,
    Json(payload): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<TaskResponse>), (StatusCode, String)> {
    let now = Utc::now();
    let task = Task {
        id: Uuid::new_v4(),
        title: payload.title,
        description: payload.description,
        status: payload.status,
        priority: payload.priority,
        category: payload.category,
        service_id: payload.service_id,
        tags: payload.tags,
        due_date: payload.due_date,
        created_by: user.id,
        assigned_to: payload.assigned_to,
        created_at: now,
        updated_at: now,
    };

    state
        .db
        .create_task(&task)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let users = load_user_names(&state)?;
    let response = task_to_response(task, &users);

    // Broadcast to WebSocket clients
    broadcast(&state, WsMessage::TaskCreated { task: response.clone() });

    Ok((StatusCode::CREATED, Json(response)))
}

// GET /api/tasks/:id
pub async fn get_task(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
) -> Result<Json<TaskResponse>, (StatusCode, String)> {
    let task = state
        .db
        .get_task(id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Task not found".to_string()))?;

    let users = load_user_names(&state)?;
    Ok(Json(task_to_response(task, &users)))
}

// PUT /api/tasks/:id
pub async fn update_task(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateTaskRequest>,
) -> Result<Json<TaskResponse>, (StatusCode, String)> {
    let mut task = state
        .db
        .get_task(id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Task not found".to_string()))?;

    // Apply updates
    if let Some(title) = payload.title {
        task.title = title;
    }
    if let Some(description) = payload.description {
        task.description = Some(description);
    }
    if let Some(status) = payload.status {
        task.status = status;
    }
    if let Some(priority) = payload.priority {
        task.priority = priority;
    }
    if let Some(category) = payload.category {
        task.category = Some(category);
    }
    if let Some(service_id) = payload.service_id {
        task.service_id = Some(service_id);
    }
    if let Some(tags) = payload.tags {
        task.tags = tags;
    }
    if let Some(due_date) = payload.due_date {
        task.due_date = Some(due_date);
    }
    if let Some(assigned_to) = payload.assigned_to {
        task.assigned_to = Some(assigned_to);
    }

    task.updated_at = Utc::now();

    state
        .db
        .update_task(&task)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let users = load_user_names(&state)?;
    let response = task_to_response(task, &users);

    // Broadcast to WebSocket clients
    broadcast(&state, WsMessage::TaskUpdated { task: response.clone() });

    Ok(Json(response))
}

// DELETE /api/tasks/:id
pub async fn delete_task(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let deleted = state
        .db
        .delete_task(id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !deleted {
        return Err((StatusCode::NOT_FOUND, "Task not found".to_string()));
    }

    // Broadcast to WebSocket clients
    broadcast(&state, WsMessage::TaskDeleted { task_id: id });

    Ok(StatusCode::NO_CONTENT)
}

// GET /api/users
pub async fn list_users(
    State(state): State<SharedState>,
) -> Result<Json<Vec<UserResponse>>, (StatusCode, String)> {
    let users = state
        .db
        .list_users()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let responses: Vec<UserResponse> = users.into_iter().map(UserResponse::from).collect();

    Ok(Json(responses))
}

// GET /api/services
pub async fn list_services(
    State(state): State<SharedState>,
) -> Result<Json<Vec<ServiceResponse>>, (StatusCode, String)> {
    let services = state
        .db
        .list_services()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let responses: Vec<ServiceResponse> = services.into_iter().map(ServiceResponse::from).collect();

    Ok(Json(responses))
}
