use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::{error::Error, sync::Arc};
use uuid::Uuid;

use crate::{settings::Settings, tao_task::TaoTask, user::User, user_add_request::UserAddRequest, user_edit_request::UserEditRequest};

const USERS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("users");
const USERNAME_INDEX: TableDefinition<&str, &[u8]> = TableDefinition::new("username_index");
const TASKS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("tasks");


#[derive(Clone)]
pub struct DataContext {
    db: Arc<Database>
}

impl DataContext {
    pub fn new(path: &str) -> Result<Self, redb::Error> {
        let db = Database::create(path)?;
        let write_txn = db.begin_write()?;
        let _ = write_txn.open_table(USERS_TABLE)?;
        let _ = write_txn.open_table(TASKS_TABLE)?;
        let _ = write_txn.open_table(USERNAME_INDEX)?;
        write_txn.commit()?;
        Ok(DataContext { db: Arc::new(db)})
    }

    // USERS
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

    pub fn delete_user(&self, id: Uuid) -> Result<bool, redb::Error> {
        let user = if let Some(user) = self.get_user(id)? { user } else { return Ok(false) };
        let write_txn = self.db.begin_write()?;
        let mut deleted;
        {
            let mut table = write_txn.open_table(USERNAME_INDEX)?;   
            deleted = table.remove(user.username.as_str())?.is_some();
        }
        {
            let mut table = write_txn.open_table(USERS_TABLE)?;
            let result = table.remove(user.id.as_bytes().as_slice())?;
            deleted = deleted || result.is_some();
        }
        write_txn.commit()?;
        Ok(deleted)
    }

    pub fn edit_user(&self, id: Uuid, dto: UserEditRequest) -> Result<bool, redb::Error> {
        let user = if let Some(u) = self.get_user(id)? { u } else { return Ok(false) };
        let write_txn = self.db.begin_write()?;
        let edited_user = user.clone().edit(dto);
        {
            let mut users_table = write_txn.open_table(USERS_TABLE)?;
            let mut username_index = write_txn.open_table(USERNAME_INDEX)?;
            let user_bytes = serde_json::to_vec(&edited_user).unwrap();
            let id_bytes = edited_user.id.as_bytes();
            users_table.insert(id_bytes.as_slice(), user_bytes.as_slice())?;
            username_index.insert(edited_user.username.as_str(), id_bytes.as_slice())?;
            if user.username != edited_user.username {
                username_index.remove(user.username.as_str())?;
            }
        }
        write_txn.commit()?;
        Ok(true)
    }

    // Initialize with a default admin user if no users exist
    pub fn ensure_default_user(&self) -> Result<(), Box<dyn Error>> {
        let settings = Settings::load()?;
        let users = self.list_users()?;
        if users.is_empty() {

            let default_user_creation_request = UserAddRequest {
                password: settings.default_admin_password.clone(),
                username: settings.default_admin_username.clone(),
                email: settings.default_admin_email.clone()
            };
            let default_admin = User::new(default_user_creation_request);
            self.create_user(&default_admin)?;
            println!("Created default admin user {}", settings.default_admin_username);
        }
        Ok(())
    }

    // TAOTASKS
    pub fn create_task(&self, task: &TaoTask) -> Result<(), redb::Error> {
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

    pub fn get_task(&self, id: Uuid) -> Result<Option<TaoTask>, redb::Error> {
        let read_txn = self.db.begin_read()?;
        let tasks_table = read_txn.open_table(TASKS_TABLE)?;

        let id_bytes = id.as_bytes();
        match tasks_table.get(id_bytes.as_slice())? {
            Some(data) => {
                let task: TaoTask = serde_json::from_slice(data.value()).unwrap();
                Ok(Some(task))
            }
            None => Ok(None),
        }
    }

    pub fn list_tasks(&self) -> Result<Vec<TaoTask>, redb::Error> {
        let read_txn = self.db.begin_read()?;
        let tasks_table = read_txn.open_table(TASKS_TABLE)?;

        let mut tasks = Vec::new();
        for entry in tasks_table.iter()? {
            let (_, value) = entry?;
            let task: TaoTask = serde_json::from_slice(value.value()).unwrap();
            tasks.push(task);
        }

        // Sort by created_at descending
        tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(tasks)
    }

    pub fn update_task(&self, task: &TaoTask) -> Result<(), redb::Error> {
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
}
