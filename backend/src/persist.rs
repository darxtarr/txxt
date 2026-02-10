//! World ↔ redb persistence.
//!
//! redb is a save file: loaded on boot, flushed on every mutation.
//! Never queried at runtime — World is the runtime truth.

use crate::world::{Event, Service, Task, User, World};
use redb::{Database, ReadableTable, TableDefinition};
use std::sync::Arc;
#[cfg(feature = "profile")]
use std::time::Instant;
use uuid::Uuid;

// New tables — separate from the old db.rs tables so both coexist during transition.
const WORLD_TASKS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("world_tasks");
const WORLD_USERS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("world_users");
const WORLD_SERVICES: TableDefinition<&[u8], &[u8]> = TableDefinition::new("world_services");
const WORLD_META: TableDefinition<&str, &[u8]> = TableDefinition::new("world_meta");

/// Thin handle to the redb file. Cloneable (Arc inside).
#[derive(Clone)]
pub struct SaveFile {
    db: Arc<Database>,
}

impl SaveFile {
    /// Open (or create) the save file at the given path.
    /// Creates tables if they don't exist.
    pub fn open(path: &str) -> Result<Self, SaveFileError> {
        let db = Database::create(path)?;

        // Ensure tables exist
        let txn = db.begin_write()?;
        {
            let _ = txn.open_table(WORLD_TASKS)?;
            let _ = txn.open_table(WORLD_USERS)?;
            let _ = txn.open_table(WORLD_SERVICES)?;
            let _ = txn.open_table(WORLD_META)?;
        }
        txn.commit()?;

        Ok(SaveFile { db: Arc::new(db) })
    }

    /// Load the entire World from disk. Called once at boot.
    pub fn load_world(&self) -> Result<World, SaveFileError> {
        let mut world = World::new();
        let txn = self.db.begin_read()?;

        // Load tasks
        let tasks_table = txn.open_table(WORLD_TASKS)?;
        for entry in tasks_table.iter()? {
            let (_, value) = entry?;
            let task: Task = postcard::from_bytes(value.value())
                .map_err(|e| SaveFileError::Decode(e.to_string()))?;
            world.tasks.insert(task.id, task);
        }

        // Load users
        let users_table = txn.open_table(WORLD_USERS)?;
        for entry in users_table.iter()? {
            let (_, value) = entry?;
            let user: User = postcard::from_bytes(value.value())
                .map_err(|e| SaveFileError::Decode(e.to_string()))?;
            world.users.insert(user.id, user);
        }

        // Load services
        let services_table = txn.open_table(WORLD_SERVICES)?;
        for entry in services_table.iter()? {
            let (_, value) = entry?;
            let service: Service = postcard::from_bytes(value.value())
                .map_err(|e| SaveFileError::Decode(e.to_string()))?;
            world.services.insert(service.id, service);
        }

        // Load revision counter
        let meta_table = txn.open_table(WORLD_META)?;
        if let Some(rev_data) = meta_table.get("revision")? {
            let bytes = rev_data.value();
            if bytes.len() == 8 {
                world.revision = u64::from_le_bytes(bytes.try_into().unwrap());
            }
        }

        Ok(world)
    }

    /// Flush a single event to disk. Called after every World::apply().
    /// Writes the affected entity + updated revision in one transaction.
    pub fn flush(&self, world: &World, event: &Event) -> Result<(), SaveFileError> {
        #[cfg(feature = "profile")]
        let total_start = Instant::now();
        let txn = self.db.begin_write()?;
        {
            #[cfg(feature = "profile")]
            let table_start = Instant::now();
            let mut tasks = txn.open_table(WORLD_TASKS)?;
            let mut meta = txn.open_table(WORLD_META)?;
            #[cfg(feature = "profile")]
            tracing::debug!(elapsed_us = table_start.elapsed().as_micros() as u64, "flush opened tables");

            #[cfg(feature = "profile")]
            let write_start = Instant::now();
            match event {
                Event::TaskCreated { task, .. } => {
                    let bytes = postcard::to_allocvec(task)
                        .map_err(|e| SaveFileError::Encode(e.to_string()))?;
                    tasks.insert(task.id.as_bytes().as_slice(), bytes.as_slice())?;
                }

                Event::TaskScheduled { task_id, .. }
                | Event::TaskMoved { task_id, .. }
                | Event::TaskUnscheduled { task_id, .. }
                | Event::TaskCompleted { task_id, .. } => {
                    // Look up the current state in World and write the whole entity
                    let task = &world.tasks[task_id];
                    let bytes = postcard::to_allocvec(task)
                        .map_err(|e| SaveFileError::Encode(e.to_string()))?;
                    tasks.insert(task_id.as_bytes().as_slice(), bytes.as_slice())?;
                }

                Event::TaskDeleted { task_id, .. } => {
                    tasks.remove(task_id.as_bytes().as_slice())?;
                }
            }

            // Always update revision
            meta.insert("revision", world.revision.to_le_bytes().as_slice())?;
            #[cfg(feature = "profile")]
            tracing::debug!(elapsed_us = write_start.elapsed().as_micros() as u64, "flush wrote rows and revision");
        }
        #[cfg(feature = "profile")]
        let commit_start = Instant::now();
        txn.commit()?;
        #[cfg(feature = "profile")]
        tracing::debug!(elapsed_us = commit_start.elapsed().as_micros() as u64, total_us = total_start.elapsed().as_micros() as u64, "flush committed transaction");
        Ok(())
    }

    /// Write a user to the save file (for seeding / account creation).
    pub fn save_user(&self, user: &User) -> Result<(), SaveFileError> {
        let txn = self.db.begin_write()?;
        {
            let mut users = txn.open_table(WORLD_USERS)?;
            let bytes = postcard::to_allocvec(user)
                .map_err(|e| SaveFileError::Encode(e.to_string()))?;
            users.insert(user.id.as_bytes().as_slice(), bytes.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    /// Write a service to the save file (for seeding).
    pub fn save_service(&self, service: &Service) -> Result<(), SaveFileError> {
        let txn = self.db.begin_write()?;
        {
            let mut services = txn.open_table(WORLD_SERVICES)?;
            let bytes = postcard::to_allocvec(service)
                .map_err(|e| SaveFileError::Encode(e.to_string()))?;
            services.insert(service.id.as_bytes().as_slice(), bytes.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    /// Seed default services if none exist. Returns how many were created.
    pub fn ensure_default_services(&self, world: &mut World) -> Result<usize, SaveFileError> {
        if !world.services.is_empty() {
            return Ok(0);
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

        for (id_str, name) in defaults {
            let service = Service {
                id: Uuid::parse_str(id_str).unwrap(),
                name: name.to_string(),
            };
            self.save_service(&service)?;
            world.services.insert(service.id, service);
        }

        Ok(defaults.len())
    }

    /// Seed default admin user if no users exist. Returns true if created.
    pub fn ensure_default_user(&self, world: &mut World) -> Result<bool, SaveFileError> {
        if !world.users.is_empty() {
            return Ok(false);
        }

        use argon2::{
            password_hash::{rand_core::OsRng, SaltString},
            Argon2, PasswordHasher,
        };

        let salt = SaltString::generate(&mut OsRng);
        let password_hash = Argon2::default()
            .hash_password(b"admin", &salt)
            .unwrap()
            .to_string();

        let user = User {
            id: Uuid::new_v4(),
            username: "admin".to_string(),
            password_hash,
        };

        self.save_user(&user)?;
        world.users.insert(user.id, user);
        Ok(true)
    }
}

// ── Errors ─────────────────────────────────────────────────────

#[derive(Debug)]
pub enum SaveFileError {
    Redb(String),
    Decode(String),
    Encode(String),
}

// redb 2.x has many error types. Blanket them all into SaveFileError::Redb.
macro_rules! from_redb {
    ($($t:ty),*) => {
        $(impl From<$t> for SaveFileError {
            fn from(e: $t) -> Self { SaveFileError::Redb(e.to_string()) }
        })*
    };
}

from_redb!(
    redb::Error,
    redb::DatabaseError,
    redb::TableError,
    redb::TransactionError,
    redb::StorageError,
    redb::CommitError
);

impl std::fmt::Display for SaveFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveFileError::Redb(e) => write!(f, "redb: {e}"),
            SaveFileError::Decode(e) => write!(f, "decode: {e}"),
            SaveFileError::Encode(e) => write!(f, "encode: {e}"),
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Command, Priority};
    use std::fs;

    /// Create a temp save file that auto-cleans.
    fn temp_save(name: &str) -> (SaveFile, String) {
        let path = format!("/tmp/txxt_test_{name}_{}.redb", std::process::id());
        let _ = fs::remove_file(&path); // clean up any leftover
        let sf = SaveFile::open(&path).unwrap();
        (sf, path)
    }

    fn cleanup(path: &str) {
        let _ = fs::remove_file(path);
    }

    #[test]
    fn round_trip_empty_world() {
        let (sf, path) = temp_save("empty");

        let world = sf.load_world().unwrap();
        assert_eq!(world.tasks.len(), 0);
        assert_eq!(world.users.len(), 0);
        assert_eq!(world.services.len(), 0);
        assert_eq!(world.revision, 0);

        cleanup(&path);
    }

    #[test]
    fn seed_and_reload() {
        let (sf, path) = temp_save("seed");

        // Boot, seed, shut down
        let mut world = sf.load_world().unwrap();
        let svc_count = sf.ensure_default_services(&mut world).unwrap();
        let user_created = sf.ensure_default_user(&mut world).unwrap();
        assert_eq!(svc_count, 12);
        assert!(user_created);

        // Reboot — data should be there
        let world2 = sf.load_world().unwrap();
        assert_eq!(world2.services.len(), 12);
        assert_eq!(world2.users.len(), 1);

        // Seed again — should be a no-op
        let mut world3 = sf.load_world().unwrap();
        assert_eq!(sf.ensure_default_services(&mut world3).unwrap(), 0);
        assert!(!sf.ensure_default_user(&mut world3).unwrap());

        cleanup(&path);
    }

    #[test]
    fn flush_and_reload_tasks() {
        let (sf, path) = temp_save("tasks");

        let mut world = sf.load_world().unwrap();
        sf.ensure_default_services(&mut world).unwrap();

        // Pick the first service
        let svc_id = *world.services.keys().next().unwrap();
        let user_id = Uuid::nil();

        // Create a task
        let event = world.apply(
            Command::CreateTask {
                title: "Test task".into(),
                service_id: svc_id,
                priority: Priority::High,
                assigned_to: None,
            },
            user_id,
        ).unwrap();
        sf.flush(&world, &event).unwrap();

        let task_id = match &event {
            Event::TaskCreated { task, .. } => task.id,
            _ => panic!("expected TaskCreated"),
        };

        // Schedule it
        let event = world.apply(
            Command::ScheduleTask {
                task_id,
                day: 2,
                start_time: 540,
                duration: 60,
            },
            user_id,
        ).unwrap();
        sf.flush(&world, &event).unwrap();

        // Reboot — world should have the task in the right state
        let world2 = sf.load_world().unwrap();
        assert_eq!(world2.revision, 2);
        assert_eq!(world2.tasks.len(), 1);

        let task = &world2.tasks[&task_id];
        assert_eq!(task.title, "Test task");
        assert_eq!(task.status, crate::world::TaskStatus::Scheduled);
        assert_eq!(task.day, Some(2));
        assert_eq!(task.start_time, Some(540));
        assert_eq!(task.duration, Some(60));

        cleanup(&path);
    }

    #[test]
    fn delete_task_removes_from_disk() {
        let (sf, path) = temp_save("delete");

        let mut world = sf.load_world().unwrap();
        sf.ensure_default_services(&mut world).unwrap();

        let svc_id = *world.services.keys().next().unwrap();

        let event = world.apply(
            Command::CreateTask {
                title: "Doomed".into(),
                service_id: svc_id,
                priority: Priority::Low,
                assigned_to: None,
            },
            Uuid::nil(),
        ).unwrap();
        sf.flush(&world, &event).unwrap();

        let task_id = match &event {
            Event::TaskCreated { task, .. } => task.id,
            _ => panic!(),
        };

        let event = world.apply(
            Command::DeleteTask { task_id },
            Uuid::nil(),
        ).unwrap();
        sf.flush(&world, &event).unwrap();

        // Reboot — task should be gone
        let world2 = sf.load_world().unwrap();
        assert_eq!(world2.tasks.len(), 0);
        assert_eq!(world2.revision, 2);

        cleanup(&path);
    }
}
