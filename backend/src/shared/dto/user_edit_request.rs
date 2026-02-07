use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserEditRequest {
    pub username: Option<String>,
    pub email: Option<String>,
}