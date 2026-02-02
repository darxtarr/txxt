use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::{task_priority::TaskPriority, task_status::TaskStatus};

#[derive(Debug, Deserialize)]
pub struct UpdateTaskRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub priority: Option<TaskPriority>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub due_date: Option<DateTime<Utc>>,
    pub assigned_to: Option<Uuid>,
}