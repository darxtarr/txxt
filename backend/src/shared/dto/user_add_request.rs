use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAddRequest {
    pub username: String,
    pub password: String,
    pub email: String
}