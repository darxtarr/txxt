use crate::models::{Service, Task, User};
use redb::{Database, ReadableTable, TableDefinition};
use std::sync::Arc;
use uuid::Uuid;

const USERS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("users");
const TASKS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("tasks");
const USERNAME_INDEX: TableDefinition<&str, &[u8]> = TableDefinition::new("username_index");
const SERVICES_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("services");

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
            let _ = write_txn.open_table(SERVICES_TABLE)?;
        }
        write_txn.commit()?;

        Ok(Db { db: Arc::new(db) })
    }

    // Service operations
    pub fn create_service(&self, service: &Service) -> Result<(), redb::Error> {
        let write_txn = self.db.begin_write()?;
        {
            let mut services_table = write_txn.open_table(SERVICES_TABLE)?;
            let service_bytes = postcard::to_allocvec(service).unwrap();
            let id_bytes = service.id.as_bytes();
            services_table.insert(id_bytes.as_slice(), service_bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn list_services(&self) -> Result<Vec<Service>, redb::Error> {
        let read_txn = self.db.begin_read()?;
        let services_table = read_txn.open_table(SERVICES_TABLE)?;

        let mut services = Vec::new();
        for entry in services_table.iter()? {
            let (_, value) = entry?;
            let service: Service = postcard::from_bytes(value.value()).unwrap();
            services.push(service);
        }
        services.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(services)
    }

    // User operations
    pub fn create_user(&self, user: &User) -> Result<(), redb::Error> {
        let write_txn = self.db.begin_write()?;
        {
            let mut users_table = write_txn.open_table(USERS_TABLE)?;
            let mut username_index = write_txn.open_table(USERNAME_INDEX)?;

            let user_bytes = postcard::to_allocvec(user).unwrap();
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
                let user: User = postcard::from_bytes(data.value()).unwrap();
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
                        let user: User = postcard::from_bytes(user_data.value()).unwrap();
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
            let user: User = postcard::from_bytes(value.value()).unwrap();
            users.push(user);
        }
        Ok(users)
    }

    // Task operations
    pub fn create_task(&self, task: &Task) -> Result<(), redb::Error> {
        let write_txn = self.db.begin_write()?;
        {
            let mut tasks_table = write_txn.open_table(TASKS_TABLE)?;
            let task_bytes = postcard::to_allocvec(task).unwrap();
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
                let task: Task = postcard::from_bytes(data.value()).unwrap();
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
            let task: Task = postcard::from_bytes(value.value()).unwrap();
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
            let task_bytes = postcard::to_allocvec(task).unwrap();
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

    pub fn ensure_default_services(&self) -> Result<(), redb::Error> {
        let services = self.list_services()?;
        if !services.is_empty() {
            return Ok(());
        }

        let defaults = [
            ("6b3c18d4-2a1d-4f2b-9d4c-0a0c3f0f2f10", "Billing Portal"),
            ("a8c2f1f0-8b8f-4a62-9d3a-8c1d7b4c2a01", "Customer Support"),
            ("2e6a7c11-8c39-4d5f-9a0e-6e1a4c7f3b22", "Data Warehouse"),
            ("d0b74f7e-3c2a-4a58-8b21-5e9d2a1c4f33", "Fraud Detection"),
            ("f2a1c3b4-5d6e-4f70-8123-4567890abcde", "Identity"),
            ("0c1d2e3f-4a5b-6c7d-8e9f-0123456789ab", "Internal Tools"),
            ("11121314-1516-1718-191a-1b1c1d1e1f20", "Mobile App"),
            ("21222324-2526-2728-292a-2b2c2d2e2f30", "Payments"),
            ("31323334-3536-3738-393a-3b3c3d3e3f40", "Reporting"),
            ("41424344-4546-4748-494a-4b4c4d4e4f50", "Search"),
            ("51525354-5556-5758-595a-5b5c5d5e5f60", "Shipping"),
            ("61626364-6566-6768-696a-6b6c6d6e6f70", "Web App"),
        ];

        for (id, name) in defaults {
            let service = Service {
                id: Uuid::parse_str(id).unwrap(),
                name: name.to_string(),
            };
            self.create_service(&service)?;
        }

        Ok(())
    }
}
