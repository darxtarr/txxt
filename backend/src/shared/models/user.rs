use argon2::{Argon2, PasswordHasher, password_hash::{SaltString, rand_core::OsRng}};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{user_add_request::UserAddRequest, user_edit_request::UserEditRequest, user_get_response::UserGetResponse};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
    pub email: String,
    pub created_at: DateTime<Utc>,
}

impl User {
    pub fn new(request: UserAddRequest) -> Self {
        Self {
            id: Uuid::new_v4(),
            username: request.username,
            email: request.email,
            created_at: Utc::now(),
            password_hash: User::get_hashed_password(request.password.trim().as_bytes())
        }
    }

    pub fn edit(self, request: UserEditRequest) -> Self {
        Self {
            id: self.id,
            username: request.username.unwrap_or(self.username),
            email: request.email.unwrap_or(self.email),
            password_hash: self.password_hash,
            created_at: self.created_at
        }
    }

    pub fn to_get_dto(&self) -> UserGetResponse {
        UserGetResponse {
            id: self.id,
            username: self.username.clone(),
            created_at: self.created_at
        }
    }

    fn get_hashed_password(password_bytes: &[u8]) -> String {
        let salt = SaltString::generate(&mut OsRng);
            let argon2 = Argon2::default();
            argon2
                .hash_password(password_bytes, &salt)
                .unwrap()
                .to_string()
    }
}