use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::{task_priority::TaskPriority, task_status::TaskStatus};

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    pub description: Option<String>,
    #[serde(default = "default_status")]
    pub status: TaskStatus,
    #[serde(default = "default_priority")]
    pub priority: TaskPriority,
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub due_date: Option<DateTime<Utc>>,
    pub assigned_to: Option<Uuid>,
}

fn default_status() -> TaskStatus {
    TaskStatus::Pending
}

fn default_priority() -> TaskPriority {
    TaskPriority::Medium
}