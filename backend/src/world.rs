use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ── Entity types ──────────────────────────────────────────────

/// Task status lifecycle: Staged → Scheduled → Active → Completed
///
/// Staged    = exists but has no time slot (lives in the staging queue)
/// Scheduled = has a day + time slot on the grid
/// Active    = being worked right now
/// Completed = done
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum TaskStatus {
    Staged = 0,
    Scheduled = 1,
    Active = 2,
    Completed = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum Priority {
    Low = 0,
    Medium = 1,
    High = 2,
    Urgent = 3,
}

/// A task — the unit of work on the scheduling grid.
///
/// Scheduling fields (date, start_time, duration) are only meaningful
/// when status is Scheduled or Active. When Staged, they're None.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub status: TaskStatus,
    pub priority: Priority,
    pub service_id: Uuid,
    pub created_by: Uuid,
    pub assigned_to: Option<Uuid>,
    /// Calendar date: days since Unix epoch (1970-01-01 = 0). None if Staged.
    /// Day-of-week is derived: (date + 3) % 7 → 0=Mon .. 6=Sun.
    pub date: Option<u16>,
    /// Minutes from midnight, snapped to 15-min grid. None if Staged.
    pub start_time: Option<u16>,
    /// Duration in minutes, snapped to 15-min grid. None if Staged.
    pub duration: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub id: Uuid,
    pub name: String,
}

// ── Commands (client → server) ────────────────────────────────

/// A command is something a client wants to happen.
/// The server validates it, applies it, and returns an Event (or an error).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    CreateTask {
        title: String,
        service_id: Uuid,
        priority: Priority,
        assigned_to: Option<Uuid>,
        /// If all three scheduling fields are Some, create directly as Scheduled.
        /// If None, create as Staged (existing behavior).
        date: Option<u16>,
        start_time: Option<u16>,
        duration: Option<u16>,
    },
    ScheduleTask {
        task_id: Uuid,
        date: u16,
        start_time: u16,
        duration: u16,
    },
    MoveTask {
        task_id: Uuid,
        date: u16,
        start_time: u16,
        duration: u16,
    },
    UnscheduleTask {
        task_id: Uuid,
    },
    CompleteTask {
        task_id: Uuid,
    },
    DeleteTask {
        task_id: Uuid,
    },
}

// ── Events (server → clients) ─────────────────────────────────

/// An event is what actually happened. Broadcast to all connected clients.
/// Each event carries the revision number it was applied at.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    TaskCreated {
        revision: u64,
        task: Task,
    },
    TaskScheduled {
        revision: u64,
        task_id: Uuid,
        date: u16,
        start_time: u16,
        duration: u16,
    },
    TaskMoved {
        revision: u64,
        task_id: Uuid,
        date: u16,
        start_time: u16,
        duration: u16,
    },
    TaskUnscheduled {
        revision: u64,
        task_id: Uuid,
    },
    TaskCompleted {
        revision: u64,
        task_id: Uuid,
    },
    TaskDeleted {
        revision: u64,
        task_id: Uuid,
    },
}

// ── Errors ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldError {
    TaskNotFound,
    ServiceNotFound,
    InvalidDate,
    InvalidTime,
    InvalidDuration,
    /// Task is already in the requested state
    InvalidTransition,
}

// ── The World ──────────────────────────────────────────────────

/// The authoritative game state. Lives in memory. Loaded from redb on boot.
/// All mutations go through apply() which validates, mutates, and returns
/// an Event for broadcast.
pub struct World {
    pub tasks: HashMap<Uuid, Task>,
    pub users: HashMap<Uuid, User>,
    pub services: HashMap<Uuid, Service>,
    pub revision: u64,
    /// Recent event log for reconnect replay and undo.
    pub log: Vec<(u64, Event)>,
}

impl World {
    pub fn new() -> Self {
        World {
            tasks: HashMap::new(),
            users: HashMap::new(),
            services: HashMap::new(),
            revision: 0,
            log: Vec::new(),
        }
    }

    /// Apply a command to the world. Returns the resulting Event on success.
    /// This is THE mutation codepath — every state change goes through here.
    pub fn apply(&mut self, cmd: Command, user_id: Uuid) -> Result<Event, WorldError> {
        match cmd {
            Command::CreateTask { title, service_id, priority, assigned_to, date, start_time, duration } => {
                // Validate: service must exist
                if !self.services.contains_key(&service_id) {
                    return Err(WorldError::ServiceNotFound);
                }

                // If all scheduling fields provided, validate and create as Scheduled
                let (status, date, start_time, duration) = match (date, start_time, duration) {
                    (Some(d), Some(st), Some(dur)) => {
                        validate_scheduling(d, st, dur)?;
                        (TaskStatus::Scheduled, Some(d), Some(st), Some(dur))
                    }
                    _ => (TaskStatus::Staged, None, None, None),
                };

                let task = Task {
                    id: Uuid::new_v4(),
                    title,
                    status,
                    priority,
                    service_id,
                    created_by: user_id,
                    assigned_to,
                    date,
                    start_time,
                    duration,
                };

                self.revision += 1;
                let event = Event::TaskCreated {
                    revision: self.revision,
                    task: task.clone(),
                };
                self.tasks.insert(task.id, task);
                self.log.push((self.revision, event.clone()));
                Ok(event)
            }

            Command::ScheduleTask { task_id, date, start_time, duration } => {
                validate_scheduling(date, start_time, duration)?;

                let task = self.tasks.get_mut(&task_id)
                    .ok_or(WorldError::TaskNotFound)?;

                // Can only schedule a Staged task
                if task.status != TaskStatus::Staged {
                    return Err(WorldError::InvalidTransition);
                }

                task.status = TaskStatus::Scheduled;
                task.date = Some(date);
                task.start_time = Some(start_time);
                task.duration = Some(duration);

                self.revision += 1;
                let event = Event::TaskScheduled {
                    revision: self.revision,
                    task_id,
                    date,
                    start_time,
                    duration,
                };
                self.log.push((self.revision, event.clone()));
                Ok(event)
            }

            Command::MoveTask { task_id, date, start_time, duration } => {
                validate_scheduling(date, start_time, duration)?;

                let task = self.tasks.get_mut(&task_id)
                    .ok_or(WorldError::TaskNotFound)?;

                // Can only move a Scheduled or Active task (something on the grid)
                if task.status != TaskStatus::Scheduled && task.status != TaskStatus::Active {
                    return Err(WorldError::InvalidTransition);
                }

                task.date = Some(date);
                task.start_time = Some(start_time);
                task.duration = Some(duration);

                self.revision += 1;
                let event = Event::TaskMoved {
                    revision: self.revision,
                    task_id,
                    date,
                    start_time,
                    duration,
                };
                self.log.push((self.revision, event.clone()));
                Ok(event)
            }

            Command::UnscheduleTask { task_id } => {
                let task = self.tasks.get_mut(&task_id)
                    .ok_or(WorldError::TaskNotFound)?;

                // Can only unschedule something that's on the grid
                if task.status != TaskStatus::Scheduled && task.status != TaskStatus::Active {
                    return Err(WorldError::InvalidTransition);
                }

                task.status = TaskStatus::Staged;
                task.date = None;
                task.start_time = None;
                task.duration = None;

                self.revision += 1;
                let event = Event::TaskUnscheduled {
                    revision: self.revision,
                    task_id,
                };
                self.log.push((self.revision, event.clone()));
                Ok(event)
            }

            Command::CompleteTask { task_id } => {
                let task = self.tasks.get_mut(&task_id)
                    .ok_or(WorldError::TaskNotFound)?;

                // Can complete from Scheduled or Active (not Staged — finish what's planned)
                if task.status != TaskStatus::Scheduled && task.status != TaskStatus::Active {
                    return Err(WorldError::InvalidTransition);
                }

                task.status = TaskStatus::Completed;

                self.revision += 1;
                let event = Event::TaskCompleted {
                    revision: self.revision,
                    task_id,
                };
                self.log.push((self.revision, event.clone()));
                Ok(event)
            }

            Command::DeleteTask { task_id } => {
                if self.tasks.remove(&task_id).is_none() {
                    return Err(WorldError::TaskNotFound);
                }

                self.revision += 1;
                let event = Event::TaskDeleted {
                    revision: self.revision,
                    task_id,
                };
                self.log.push((self.revision, event.clone()));
                Ok(event)
            }
        }
    }

    /// Look up a user by username (linear scan — fine for 5-20 users).
    pub fn get_user_by_username(&self, username: &str) -> Option<&User> {
        self.users.values().find(|u| u.username == username)
    }

    /// Get all Staged tasks, sorted by priority (highest first).
    /// This is the staging queue that IRONCLAD renders as a sidebar list.
    pub fn staging_queue(&self) -> Vec<&Task> {
        let mut staged: Vec<&Task> = self.tasks.values()
            .filter(|t| t.status == TaskStatus::Staged)
            .collect();
        // Sort by priority descending (Urgent first, Low last)
        staged.sort_by(|a, b| b.priority.cmp(&a.priority));
        staged
    }

    /// Get all events since a given revision (for reconnect replay).
    /// Returns None if the revision is too old (caller should send full snapshot).
    pub fn events_since(&self, since_rev: u64) -> Option<&[(u64, Event)]> {
        // Find the first log entry after since_rev
        let start = self.log.iter().position(|(rev, _)| *rev > since_rev);
        match start {
            Some(idx) => Some(&self.log[idx..]),
            None if since_rev >= self.revision => Some(&[]), // up to date
            None => None, // too old, log was trimmed
        }
    }
}

// ── Validation helpers ─────────────────────────────────────────

/// Validate scheduling fields.
///
/// date: epoch days (any value except 0xFFFF which is the staged sentinel)
/// start_time: minutes from midnight, must be on 15-min grid
/// duration: minutes, must be on 15-min grid, must not overflow past midnight
fn validate_scheduling(date: u16, start_time: u16, duration: u16) -> Result<(), WorldError> {
    if date == 0xFFFF {
        return Err(WorldError::InvalidDate);
    }
    // 24 hours = 1440 minutes. Must be on 15-min grid.
    if start_time >= 1440 || start_time % 15 != 0 {
        return Err(WorldError::InvalidTime);
    }
    // Duration: at least 15 min, on 15-min grid, doesn't go past midnight
    if duration == 0 || duration % 15 != 0 || start_time + duration > 1440 {
        return Err(WorldError::InvalidDuration);
    }
    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // A known Wednesday (2026-02-11). Use this as a representative test date.
    const D: u16 = 20495;
    const D2: u16 = 20496; // Thursday 2026-02-12

    fn test_world() -> World {
        let mut w = World::new();
        w.services.insert(
            Uuid::nil(),
            Service { id: Uuid::nil(), name: "Test Service".into() },
        );
        w
    }

    fn create_task(w: &mut World) -> Uuid {
        let event = w.apply(
            Command::CreateTask {
                title: "Fix the thing".into(),
                service_id: Uuid::nil(),
                priority: Priority::Medium,
                assigned_to: None,
                date: None,
                start_time: None,
                duration: None,
            },
            Uuid::nil(),
        ).unwrap();

        match event {
            Event::TaskCreated { task, .. } => task.id,
            _ => panic!("expected TaskCreated"),
        }
    }

    #[test]
    fn create_task_starts_staged() {
        let mut w = test_world();
        let id = create_task(&mut w);

        let task = &w.tasks[&id];
        assert_eq!(task.status, TaskStatus::Staged);
        assert_eq!(task.date, None);
        assert_eq!(task.start_time, None);
        assert_eq!(w.revision, 1);
    }

    #[test]
    fn create_task_with_scheduling() {
        let mut w = test_world();
        let event = w.apply(
            Command::CreateTask {
                title: "New task".into(),
                service_id: Uuid::nil(),
                priority: Priority::Medium,
                assigned_to: None,
                date: Some(D),
                start_time: Some(540),
                duration: Some(30),
            },
            Uuid::nil(),
        ).unwrap();

        let id = match event {
            Event::TaskCreated { task, .. } => task.id,
            _ => panic!("expected TaskCreated"),
        };

        let task = &w.tasks[&id];
        assert_eq!(task.status, TaskStatus::Scheduled);
        assert_eq!(task.date, Some(D));
        assert_eq!(task.start_time, Some(540));
        assert_eq!(task.duration, Some(30));
    }

    #[test]
    fn create_task_with_staged_sentinel_rejected() {
        // 0xFFFF is the staged sentinel — passing it as a date is invalid
        let mut w = test_world();
        let result = w.apply(
            Command::CreateTask {
                title: "Bad".into(),
                service_id: Uuid::nil(),
                priority: Priority::Medium,
                assigned_to: None,
                date: Some(0xFFFF),
                start_time: Some(540),
                duration: Some(30),
            },
            Uuid::nil(),
        );
        assert_eq!(result.unwrap_err(), WorldError::InvalidDate);
    }

    #[test]
    fn create_task_requires_valid_service() {
        let mut w = World::new(); // no services
        let result = w.apply(
            Command::CreateTask {
                title: "Orphan".into(),
                service_id: Uuid::new_v4(),
                priority: Priority::Low,
                assigned_to: None,
                date: None,
                start_time: None,
                duration: None,
            },
            Uuid::nil(),
        );
        assert_eq!(result.unwrap_err(), WorldError::ServiceNotFound);
        assert_eq!(w.revision, 0); // nothing changed
    }

    #[test]
    fn schedule_staged_task() {
        let mut w = test_world();
        let id = create_task(&mut w);

        w.apply(
            Command::ScheduleTask { task_id: id, date: D, start_time: 540, duration: 60 },
            Uuid::nil(),
        ).unwrap();

        let task = &w.tasks[&id];
        assert_eq!(task.status, TaskStatus::Scheduled);
        assert_eq!(task.date, Some(D));
        assert_eq!(task.start_time, Some(540)); // 9:00 AM
        assert_eq!(task.duration, Some(60));    // 1 hour
        assert_eq!(w.revision, 2);
    }

    #[test]
    fn cannot_schedule_already_scheduled() {
        let mut w = test_world();
        let id = create_task(&mut w);

        w.apply(
            Command::ScheduleTask { task_id: id, date: D, start_time: 480, duration: 30 },
            Uuid::nil(),
        ).unwrap();

        let result = w.apply(
            Command::ScheduleTask { task_id: id, date: D2, start_time: 600, duration: 30 },
            Uuid::nil(),
        );
        assert_eq!(result.unwrap_err(), WorldError::InvalidTransition);
    }

    #[test]
    fn move_scheduled_task() {
        let mut w = test_world();
        let id = create_task(&mut w);

        w.apply(
            Command::ScheduleTask { task_id: id, date: D, start_time: 480, duration: 60 },
            Uuid::nil(),
        ).unwrap();

        w.apply(
            Command::MoveTask { task_id: id, date: D2, start_time: 840, duration: 90 },
            Uuid::nil(),
        ).unwrap();

        let task = &w.tasks[&id];
        assert_eq!(task.date, Some(D2));
        assert_eq!(task.start_time, Some(840)); // 2:00 PM
        assert_eq!(task.duration, Some(90));    // 1.5 hours
        assert_eq!(w.revision, 3);
    }

    #[test]
    fn cannot_move_staged_task() {
        let mut w = test_world();
        let id = create_task(&mut w);

        let result = w.apply(
            Command::MoveTask { task_id: id, date: D, start_time: 480, duration: 60 },
            Uuid::nil(),
        );
        assert_eq!(result.unwrap_err(), WorldError::InvalidTransition);
    }

    #[test]
    fn unschedule_puts_task_back_in_staging() {
        let mut w = test_world();
        let id = create_task(&mut w);

        w.apply(
            Command::ScheduleTask { task_id: id, date: D, start_time: 600, duration: 30 },
            Uuid::nil(),
        ).unwrap();

        w.apply(Command::UnscheduleTask { task_id: id }, Uuid::nil()).unwrap();

        let task = &w.tasks[&id];
        assert_eq!(task.status, TaskStatus::Staged);
        assert_eq!(task.date, None);
        assert_eq!(task.start_time, None);
        assert_eq!(task.duration, None);
    }

    #[test]
    fn complete_scheduled_task() {
        let mut w = test_world();
        let id = create_task(&mut w);

        w.apply(
            Command::ScheduleTask { task_id: id, date: D, start_time: 480, duration: 60 },
            Uuid::nil(),
        ).unwrap();

        w.apply(Command::CompleteTask { task_id: id }, Uuid::nil()).unwrap();

        assert_eq!(w.tasks[&id].status, TaskStatus::Completed);
    }

    #[test]
    fn cannot_complete_staged_task() {
        let mut w = test_world();
        let id = create_task(&mut w);

        let result = w.apply(Command::CompleteTask { task_id: id }, Uuid::nil());
        assert_eq!(result.unwrap_err(), WorldError::InvalidTransition);
    }

    #[test]
    fn delete_task() {
        let mut w = test_world();
        let id = create_task(&mut w);

        w.apply(Command::DeleteTask { task_id: id }, Uuid::nil()).unwrap();

        assert!(!w.tasks.contains_key(&id));
    }

    #[test]
    fn delete_nonexistent_task() {
        let mut w = test_world();
        let result = w.apply(
            Command::DeleteTask { task_id: Uuid::new_v4() },
            Uuid::nil(),
        );
        assert_eq!(result.unwrap_err(), WorldError::TaskNotFound);
    }

    #[test]
    fn staging_queue_sorted_by_priority() {
        let mut w = test_world();
        let user = Uuid::nil();

        w.apply(Command::CreateTask {
            title: "Low".into(), service_id: Uuid::nil(),
            priority: Priority::Low, assigned_to: None,
            date: None, start_time: None, duration: None,
        }, user).unwrap();

        w.apply(Command::CreateTask {
            title: "Urgent".into(), service_id: Uuid::nil(),
            priority: Priority::Urgent, assigned_to: None,
            date: None, start_time: None, duration: None,
        }, user).unwrap();

        w.apply(Command::CreateTask {
            title: "High".into(), service_id: Uuid::nil(),
            priority: Priority::High, assigned_to: None,
            date: None, start_time: None, duration: None,
        }, user).unwrap();

        let queue = w.staging_queue();
        assert_eq!(queue.len(), 3);
        assert_eq!(queue[0].priority, Priority::Urgent);
        assert_eq!(queue[1].priority, Priority::High);
        assert_eq!(queue[2].priority, Priority::Low);
    }

    #[test]
    fn scheduling_validation() {
        let mut w = test_world();
        let id = create_task(&mut w);

        // Staged sentinel (0xFFFF) is not a valid date
        let r = w.apply(
            Command::ScheduleTask { task_id: id, date: 0xFFFF, start_time: 480, duration: 60 },
            Uuid::nil(),
        );
        assert_eq!(r.unwrap_err(), WorldError::InvalidDate);

        // Time not on 15-min grid
        let r = w.apply(
            Command::ScheduleTask { task_id: id, date: D, start_time: 487, duration: 60 },
            Uuid::nil(),
        );
        assert_eq!(r.unwrap_err(), WorldError::InvalidTime);

        // Duration zero
        let r = w.apply(
            Command::ScheduleTask { task_id: id, date: D, start_time: 480, duration: 0 },
            Uuid::nil(),
        );
        assert_eq!(r.unwrap_err(), WorldError::InvalidDuration);

        // Goes past midnight
        let r = w.apply(
            Command::ScheduleTask { task_id: id, date: D, start_time: 1380, duration: 120 },
            Uuid::nil(),
        );
        assert_eq!(r.unwrap_err(), WorldError::InvalidDuration);
    }

    #[test]
    fn revision_increments_on_every_mutation() {
        let mut w = test_world();
        assert_eq!(w.revision, 0);

        let id = create_task(&mut w);
        assert_eq!(w.revision, 1);

        w.apply(
            Command::ScheduleTask { task_id: id, date: D, start_time: 480, duration: 60 },
            Uuid::nil(),
        ).unwrap();
        assert_eq!(w.revision, 2);

        w.apply(
            Command::MoveTask { task_id: id, date: D2, start_time: 600, duration: 30 },
            Uuid::nil(),
        ).unwrap();
        assert_eq!(w.revision, 3);

        w.apply(Command::CompleteTask { task_id: id }, Uuid::nil()).unwrap();
        assert_eq!(w.revision, 4);
    }

    #[test]
    fn event_log_tracks_history() {
        let mut w = test_world();
        let id = create_task(&mut w);

        w.apply(
            Command::ScheduleTask { task_id: id, date: D, start_time: 480, duration: 60 },
            Uuid::nil(),
        ).unwrap();

        assert_eq!(w.log.len(), 2);
        assert_eq!(w.log[0].0, 1); // rev 1 = create
        assert_eq!(w.log[1].0, 2); // rev 2 = schedule
    }

    #[test]
    fn events_since_for_reconnect() {
        let mut w = test_world();
        create_task(&mut w); // rev 1
        create_task(&mut w); // rev 2
        create_task(&mut w); // rev 3

        // Client last saw rev 1, needs events 2 and 3
        let events = w.events_since(1).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, 2);
        assert_eq!(events[1].0, 3);

        // Client is up to date
        let events = w.events_since(3).unwrap();
        assert_eq!(events.len(), 0);

        // Client at rev 0, needs everything
        let events = w.events_since(0).unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn failed_commands_dont_change_state() {
        let mut w = test_world();
        let rev_before = w.revision;
        let log_len_before = w.log.len();

        // Try to delete a task that doesn't exist
        let _ = w.apply(
            Command::DeleteTask { task_id: Uuid::new_v4() },
            Uuid::nil(),
        );

        assert_eq!(w.revision, rev_before);
        assert_eq!(w.log.len(), log_len_before);
    }
}
