use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{task_priority::TaskPriority, task_status::TaskStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaoTaskActionResponse {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub due_date: Option<DateTime<Utc>>,
    pub created_by: Uuid,
    pub assigned_to: Option<Uuid>,
    pub assigned_to_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}