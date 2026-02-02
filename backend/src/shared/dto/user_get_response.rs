use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserGetResponse {
    pub id: Uuid,
    pub username: String,
    pub created_at: DateTime<Utc>,
}