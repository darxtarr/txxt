use crate::models::{Task, User};
use redb::{Database, ReadableTable, TableDefinition};
use std::sync::Arc;
use uuid::Uuid;

const USERS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("users");
const TASKS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("tasks");
const USERNAME_INDEX: TableDefinition<&str, &[u8]> = TableDefinition::new("username_index");

#[derive(Clone)]
pub struct Db {
    db: Arc<Database>,
}

impl Db {
    pub fn new(path: &str) -> Result<Self, redb::Error> {
        let db = Database::create(path)?;

        // Initialize tables
        let write_txn = db.begin_write()?;
        {
            let _ = write_txn.open_table(USERS_TABLE)?;
            let _ = write_txn.open_table(TASKS_TABLE)?;
            let _ = write_txn.open_table(USERNAME_INDEX)?;
        }
        write_txn.commit()?;

        Ok(Db { db: Arc::new(db) })
    }

    // User operations
    pub fn create_user(&self, user: &User) -> Result<(), redb::Error> {
        let write_txn = self.db.begin_write()?;
        {
            let mut users_table = write_txn.open_table(USERS_TABLE)?;
            let mut username_index = write_txn.open_table(USERNAME_INDEX)?;

            let user_bytes = serde_json::to_vec(user).unwrap();
            let id_bytes = user.id.as_bytes();

            users_table.insert(id_bytes.as_slice(), user_bytes.as_slice())?;
            username_index.insert(user.username.as_str(), id_bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_user(&self, id: Uuid) -> Result<Option<User>, redb::Error> {
        let read_txn = self.db.begin_read()?;
        let users_table = read_txn.open_table(USERS_TABLE)?;

        let id_bytes = id.as_bytes();
        match users_table.get(id_bytes.as_slice())? {
            Some(data) => {
                let user: User = serde_json::from_slice(data.value()).unwrap();
                Ok(Some(user))
            }
            None => Ok(None),
        }
    }

    pub fn get_user_by_username(&self, username: &str) -> Result<Option<User>, redb::Error> {
        let read_txn = self.db.begin_read()?;
        let username_index = read_txn.open_table(USERNAME_INDEX)?;

        match username_index.get(username)? {
            Some(id_data) => {
                let users_table = read_txn.open_table(USERS_TABLE)?;
                match users_table.get(id_data.value())? {
                    Some(user_data) => {
                        let user: User = serde_json::from_slice(user_data.value()).unwrap();
                        Ok(Some(user))
                    }
                    None => Ok(None),
                }
            }
            None => Ok(None),
        }
    }

    pub fn list_users(&self) -> Result<Vec<User>, redb::Error> {
        let read_txn = self.db.begin_read()?;
        let users_table = read_txn.open_table(USERS_TABLE)?;

        let mut users = Vec::new();
        for entry in users_table.iter()? {
            let (_, value) = entry?;
            let user: User = serde_json::from_slice(value.value()).unwrap();
            users.push(user);
        }
        Ok(users)
    }

    // Task operations
    pub fn create_task(&self, task: &Task) -> Result<(), redb::Error> {
        let write_txn = self.db.begin_write()?;
        {
            let mut tasks_table = write_txn.open_table(TASKS_TABLE)?;
            let task_bytes = serde_json::to_vec(task).unwrap();
            let id_bytes = task.id.as_bytes();
            tasks_table.insert(id_bytes.as_slice(), task_bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_task(&self, id: Uuid) -> Result<Option<Task>, redb::Error> {
        let read_txn = self.db.begin_read()?;
        let tasks_table = read_txn.open_table(TASKS_TABLE)?;

        let id_bytes = id.as_bytes();
        match tasks_table.get(id_bytes.as_slice())? {
            Some(data) => {
                let task: Task = serde_json::from_slice(data.value()).unwrap();
                Ok(Some(task))
            }
            None => Ok(None),
        }
    }

    pub fn list_tasks(&self) -> Result<Vec<Task>, redb::Error> {
        let read_txn = self.db.begin_read()?;
        let tasks_table = read_txn.open_table(TASKS_TABLE)?;

        let mut tasks = Vec::new();
        for entry in tasks_table.iter()? {
            let (_, value) = entry?;
            let task: Task = serde_json::from_slice(value.value()).unwrap();
            tasks.push(task);
        }

        // Sort by created_at descending
        tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(tasks)
    }

    pub fn update_task(&self, task: &Task) -> Result<(), redb::Error> {
        let write_txn = self.db.begin_write()?;
        {
            let mut tasks_table = write_txn.open_table(TASKS_TABLE)?;
            let task_bytes = serde_json::to_vec(task).unwrap();
            let id_bytes = task.id.as_bytes();
            tasks_table.insert(id_bytes.as_slice(), task_bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn delete_task(&self, id: Uuid) -> Result<bool, redb::Error> {
        let write_txn = self.db.begin_write()?;
        let deleted;
        {
            let mut tasks_table = write_txn.open_table(TASKS_TABLE)?;
            let id_bytes = id.as_bytes();
            let result = tasks_table.remove(id_bytes.as_slice())?;
            deleted = result.is_some();
        }
        write_txn.commit()?;
        Ok(deleted)
    }

    // Initialize with a default admin user if no users exist
    pub fn ensure_default_user(&self) -> Result<(), redb::Error> {
        let users = self.list_users()?;
        if users.is_empty() {
            use argon2::{
                password_hash::{rand_core::OsRng, SaltString},
                Argon2, PasswordHasher,
            };
            use chrono::Utc;

            let salt = SaltString::generate(&mut OsRng);
            let argon2 = Argon2::default();
            let password_hash = argon2
                .hash_password(b"admin", &salt)
                .unwrap()
                .to_string();

            let admin_user = User {
                id: Uuid::new_v4(),
                username: "admin".to_string(),
                password_hash,
                created_at: Utc::now(),
            };

            self.create_user(&admin_user)?;
            println!("Created default admin user (username: admin, password: admin)");
        }
        Ok(())
    }
}
