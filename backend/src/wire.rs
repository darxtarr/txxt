//! Binary wire protocol for the game WebSocket.
//!
//! Fixed-stride packed records, readable by JS DataView at known offsets.
//! No variable-length encoding. Titles are fixed-size zero-padded UTF-8.
//!
//! This is THE serialization format for all game data over WebSocket.
//! JSON is never used in the data path. Postcard is only used for
//! redb persistence (persist.rs), not for the wire.

use crate::world::{Command, Event, Priority, Task, Service, World};
use uuid::Uuid;

// ── Layout constants ───────────────────────────────────────────
// These are shared knowledge between server and client.
// The JS side hardcodes the same values.

// Message types (first byte of every WS frame)
pub mod msg {
    // Server → Client
    pub const SNAPSHOT: u8        = 0x01;
    pub const TASK_CREATED: u8    = 0x02;
    pub const TASK_SCHEDULED: u8  = 0x03;
    pub const TASK_MOVED: u8      = 0x04;
    pub const TASK_UNSCHEDULED: u8 = 0x05;
    pub const TASK_COMPLETED: u8  = 0x06;
    pub const TASK_DELETED: u8    = 0x07;
    pub const ERROR: u8           = 0xFF;

    // Client → Server
    pub const CMD_CREATE_TASK: u8    = 0x10;
    pub const CMD_SCHEDULE_TASK: u8  = 0x11;
    pub const CMD_MOVE_TASK: u8      = 0x12;
    pub const CMD_UNSCHEDULE_TASK: u8 = 0x13;
    pub const CMD_COMPLETE_TASK: u8  = 0x14;
    pub const CMD_DELETE_TASK: u8    = 0x15;
}

/// Task record stride (bytes).
///
/// ```text
/// [0..16]    id (UUID, 16 bytes)
/// [16]       status (u8: 0=Staged, 1=Scheduled, 2=Active, 3=Completed)
/// [17]       priority (u8: 0=Low, 1=Medium, 2=High, 3=Urgent)
/// [18]       day (u8: 0-6 Mon-Sun, 0xFF = not scheduled)
/// [19]       _pad
/// [20..22]   start_time (u16 LE, minutes from midnight)
/// [22..24]   duration (u16 LE, minutes)
/// [24..40]   service_id (UUID, 16 bytes)
/// [40..56]   assigned_to (UUID, 16 bytes, zeroed = unassigned)
/// [56..184]  title (128 bytes, UTF-8, zero-padded)
/// [184..192] _reserved
/// ```
pub const TASK_STRIDE: usize = 192;
pub const TITLE_MAX: usize = 128;

/// Service record stride (bytes).
///
/// ```text
/// [0..16]    id (UUID, 16 bytes)
/// [16..80]   name (64 bytes, UTF-8, zero-padded)
/// ```
pub const SERVICE_STRIDE: usize = 80;
pub const SERVICE_NAME_MAX: usize = 64;

/// Snapshot header size (bytes).
///
/// ```text
/// [0]        msg type (0x01)
/// [1..9]     revision (u64 LE)
/// [9..13]    task_count (u32 LE)
/// [13..17]   service_count (u32 LE)
/// [17..]     task records, then service records
/// ```
pub const SNAPSHOT_HEADER: usize = 17;

/// Delta event header size (bytes).
///
/// ```text
/// [0]        msg type
/// [1..9]     revision (u64 LE)
/// [9..25]    task_id (UUID, 16 bytes)
/// [25..]     event-specific payload
/// ```
pub const EVENT_HEADER: usize = 25;

// ── Packing (Server → Client) ──────────────────────────────────

/// Pack a full world snapshot into a binary frame.
pub fn pack_snapshot(world: &World) -> Vec<u8> {
    let task_count = world.tasks.len();
    let service_count = world.services.len();
    let size = SNAPSHOT_HEADER
        + task_count * TASK_STRIDE
        + service_count * SERVICE_STRIDE;

    let mut buf = vec![0u8; size];

    // Header
    buf[0] = msg::SNAPSHOT;
    buf[1..9].copy_from_slice(&world.revision.to_le_bytes());
    buf[9..13].copy_from_slice(&(task_count as u32).to_le_bytes());
    buf[13..17].copy_from_slice(&(service_count as u32).to_le_bytes());

    // Task records
    let mut offset = SNAPSHOT_HEADER;
    for task in world.tasks.values() {
        pack_task(&mut buf[offset..offset + TASK_STRIDE], task);
        offset += TASK_STRIDE;
    }

    // Service records
    for service in world.services.values() {
        pack_service(&mut buf[offset..offset + SERVICE_STRIDE], service);
        offset += SERVICE_STRIDE;
    }

    buf
}

/// Pack a single task into a fixed-stride record.
fn pack_task(buf: &mut [u8], task: &Task) {
    buf[0..16].copy_from_slice(task.id.as_bytes());
    buf[16] = task.status as u8;
    buf[17] = task.priority as u8;
    buf[18] = task.day.unwrap_or(0xFF);
    buf[19] = 0; // pad
    buf[20..22].copy_from_slice(&task.start_time.unwrap_or(0).to_le_bytes());
    buf[22..24].copy_from_slice(&task.duration.unwrap_or(0).to_le_bytes());
    buf[24..40].copy_from_slice(task.service_id.as_bytes());
    buf[40..56].copy_from_slice(
        task.assigned_to.unwrap_or(Uuid::nil()).as_bytes(),
    );
    // Title: truncate to TITLE_MAX, zero-pad
    let title_bytes = task.title.as_bytes();
    let len = title_bytes.len().min(TITLE_MAX);
    buf[56..56 + len].copy_from_slice(&title_bytes[..len]);
    // Rest is already zeroed (vec![0u8; ...])
}

/// Pack a single service into a fixed-stride record.
fn pack_service(buf: &mut [u8], service: &Service) {
    buf[0..16].copy_from_slice(service.id.as_bytes());
    let name_bytes = service.name.as_bytes();
    let len = name_bytes.len().min(SERVICE_NAME_MAX);
    buf[16..16 + len].copy_from_slice(&name_bytes[..len]);
}

/// Pack an event into a binary frame.
pub fn pack_event(event: &Event) -> Vec<u8> {
    match event {
        Event::TaskCreated { revision, task } => {
            let mut buf = vec![0u8; 1 + 8 + TASK_STRIDE];
            buf[0] = msg::TASK_CREATED;
            buf[1..9].copy_from_slice(&revision.to_le_bytes());
            pack_task(&mut buf[9..9 + TASK_STRIDE], task);
            buf
        }

        Event::TaskScheduled { revision, task_id, day, start_time, duration } => {
            let mut buf = vec![0u8; EVENT_HEADER + 5];
            buf[0] = msg::TASK_SCHEDULED;
            buf[1..9].copy_from_slice(&revision.to_le_bytes());
            buf[9..25].copy_from_slice(task_id.as_bytes());
            buf[25] = *day;
            buf[26..28].copy_from_slice(&start_time.to_le_bytes());
            buf[28..30].copy_from_slice(&duration.to_le_bytes());
            buf
        }

        Event::TaskMoved { revision, task_id, day, start_time, duration } => {
            let mut buf = vec![0u8; EVENT_HEADER + 5];
            buf[0] = msg::TASK_MOVED;
            buf[1..9].copy_from_slice(&revision.to_le_bytes());
            buf[9..25].copy_from_slice(task_id.as_bytes());
            buf[25] = *day;
            buf[26..28].copy_from_slice(&start_time.to_le_bytes());
            buf[28..30].copy_from_slice(&duration.to_le_bytes());
            buf
        }

        Event::TaskUnscheduled { revision, task_id } => {
            let mut buf = vec![0u8; EVENT_HEADER];
            buf[0] = msg::TASK_UNSCHEDULED;
            buf[1..9].copy_from_slice(&revision.to_le_bytes());
            buf[9..25].copy_from_slice(task_id.as_bytes());
            buf
        }

        Event::TaskCompleted { revision, task_id } => {
            let mut buf = vec![0u8; EVENT_HEADER];
            buf[0] = msg::TASK_COMPLETED;
            buf[1..9].copy_from_slice(&revision.to_le_bytes());
            buf[9..25].copy_from_slice(task_id.as_bytes());
            buf
        }

        Event::TaskDeleted { revision, task_id } => {
            let mut buf = vec![0u8; EVENT_HEADER];
            buf[0] = msg::TASK_DELETED;
            buf[1..9].copy_from_slice(&revision.to_le_bytes());
            buf[9..25].copy_from_slice(task_id.as_bytes());
            buf
        }
    }
}

// ── Unpacking (Client → Server) ────────────────────────────────

/// Unpack a binary command frame from the client.
pub fn unpack_command(data: &[u8]) -> Result<Command, WireError> {
    if data.is_empty() {
        return Err(WireError::TooShort);
    }

    match data[0] {
        msg::CMD_CREATE_TASK => {
            // [0]      msg type
            // [1]      priority (u8)
            // [2..18]  service_id (UUID)
            // [18..34] assigned_to (UUID, zeroed = none)
            // [34..]   title (rest of frame, UTF-8, trimmed)
            if data.len() < 34 {
                return Err(WireError::TooShort);
            }
            let priority = priority_from_u8(data[1])?;
            let service_id = uuid_from_bytes(&data[2..18]);
            let assigned_to = {
                let uuid = uuid_from_bytes(&data[18..34]);
                if uuid.is_nil() { None } else { Some(uuid) }
            };
            let title = string_from_bytes(&data[34..])?;

            Ok(Command::CreateTask { title, service_id, priority, assigned_to })
        }

        msg::CMD_SCHEDULE_TASK => {
            // [0]      msg type
            // [1..17]  task_id (UUID)
            // [17]     day (u8)
            // [18..20] start_time (u16 LE)
            // [20..22] duration (u16 LE)
            if data.len() < 22 {
                return Err(WireError::TooShort);
            }
            let task_id = uuid_from_bytes(&data[1..17]);
            let day = data[17];
            let start_time = u16::from_le_bytes([data[18], data[19]]);
            let duration = u16::from_le_bytes([data[20], data[21]]);

            Ok(Command::ScheduleTask { task_id, day, start_time, duration })
        }

        msg::CMD_MOVE_TASK => {
            // Same layout as ScheduleTask
            if data.len() < 22 {
                return Err(WireError::TooShort);
            }
            let task_id = uuid_from_bytes(&data[1..17]);
            let day = data[17];
            let start_time = u16::from_le_bytes([data[18], data[19]]);
            let duration = u16::from_le_bytes([data[20], data[21]]);

            Ok(Command::MoveTask { task_id, day, start_time, duration })
        }

        msg::CMD_UNSCHEDULE_TASK => {
            // [0]     msg type
            // [1..17] task_id (UUID)
            if data.len() < 17 {
                return Err(WireError::TooShort);
            }
            let task_id = uuid_from_bytes(&data[1..17]);
            Ok(Command::UnscheduleTask { task_id })
        }

        msg::CMD_COMPLETE_TASK => {
            if data.len() < 17 {
                return Err(WireError::TooShort);
            }
            let task_id = uuid_from_bytes(&data[1..17]);
            Ok(Command::CompleteTask { task_id })
        }

        msg::CMD_DELETE_TASK => {
            if data.len() < 17 {
                return Err(WireError::TooShort);
            }
            let task_id = uuid_from_bytes(&data[1..17]);
            Ok(Command::DeleteTask { task_id })
        }

        other => Err(WireError::UnknownMessage(other)),
    }
}

// ── Helpers ────────────────────────────────────────────────────

fn uuid_from_bytes(b: &[u8]) -> Uuid {
    Uuid::from_bytes(b[..16].try_into().unwrap())
}

fn priority_from_u8(b: u8) -> Result<Priority, WireError> {
    match b {
        0 => Ok(Priority::Low),
        1 => Ok(Priority::Medium),
        2 => Ok(Priority::High),
        3 => Ok(Priority::Urgent),
        _ => Err(WireError::InvalidField("priority")),
    }
}

fn string_from_bytes(b: &[u8]) -> Result<String, WireError> {
    // Trim trailing zeroes, then decode UTF-8
    let end = b.iter().rposition(|&c| c != 0).map_or(0, |i| i + 1);
    std::str::from_utf8(&b[..end])
        .map(|s| s.to_string())
        .map_err(|_| WireError::InvalidUtf8)
}

// ── Errors ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    TooShort,
    UnknownMessage(u8),
    InvalidField(&'static str),
    InvalidUtf8,
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireError::TooShort => write!(f, "frame too short"),
            WireError::UnknownMessage(b) => write!(f, "unknown message type: 0x{b:02X}"),
            WireError::InvalidField(name) => write!(f, "invalid field: {name}"),
            WireError::InvalidUtf8 => write!(f, "invalid UTF-8 in string field"),
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Command, Priority, TaskStatus};

    fn make_task() -> Task {
        Task {
            id: Uuid::from_bytes([1; 16]),
            title: "Deploy the widget".into(),
            status: TaskStatus::Scheduled,
            priority: Priority::High,
            service_id: Uuid::from_bytes([2; 16]),
            created_by: Uuid::from_bytes([3; 16]),
            assigned_to: Some(Uuid::from_bytes([4; 16])),
            day: Some(3),
            start_time: Some(540),
            duration: Some(90),
        }
    }

    fn make_service() -> Service {
        Service {
            id: Uuid::from_bytes([5; 16]),
            name: "Billing Portal".into(),
        }
    }

    #[test]
    fn snapshot_round_trip_layout() {
        let mut world = World::new();
        let task = make_task();
        let service = make_service();
        world.tasks.insert(task.id, task.clone());
        world.services.insert(service.id, service.clone());
        world.revision = 42;

        let buf = pack_snapshot(&world);

        // Header
        assert_eq!(buf[0], msg::SNAPSHOT);
        let rev = u64::from_le_bytes(buf[1..9].try_into().unwrap());
        assert_eq!(rev, 42);
        let task_count = u32::from_le_bytes(buf[9..13].try_into().unwrap());
        assert_eq!(task_count, 1);
        let svc_count = u32::from_le_bytes(buf[13..17].try_into().unwrap());
        assert_eq!(svc_count, 1);

        // Task record at offset 17
        let t = &buf[SNAPSHOT_HEADER..SNAPSHOT_HEADER + TASK_STRIDE];
        assert_eq!(&t[0..16], &[1u8; 16]); // id
        assert_eq!(t[16], TaskStatus::Scheduled as u8);
        assert_eq!(t[17], Priority::High as u8);
        assert_eq!(t[18], 3); // day
        let start = u16::from_le_bytes([t[20], t[21]]);
        assert_eq!(start, 540); // 9:00 AM
        let dur = u16::from_le_bytes([t[22], t[23]]);
        assert_eq!(dur, 90);
        assert_eq!(&t[24..40], &[2u8; 16]); // service_id
        assert_eq!(&t[40..56], &[4u8; 16]); // assigned_to
        let title = string_from_bytes(&t[56..184]).unwrap();
        assert_eq!(title, "Deploy the widget");

        // Service record
        let s = &buf[SNAPSHOT_HEADER + TASK_STRIDE..];
        assert_eq!(&s[0..16], &[5u8; 16]); // id
        let name = string_from_bytes(&s[16..80]).unwrap();
        assert_eq!(name, "Billing Portal");

        // Total size
        assert_eq!(buf.len(), SNAPSHOT_HEADER + TASK_STRIDE + SERVICE_STRIDE);
    }

    #[test]
    fn event_pack_task_moved() {
        let event = Event::TaskMoved {
            revision: 7,
            task_id: Uuid::from_bytes([0xAA; 16]),
            day: 4,
            start_time: 840,
            duration: 60,
        };

        let buf = pack_event(&event);
        assert_eq!(buf[0], msg::TASK_MOVED);
        let rev = u64::from_le_bytes(buf[1..9].try_into().unwrap());
        assert_eq!(rev, 7);
        assert_eq!(&buf[9..25], &[0xAA; 16]); // task_id
        assert_eq!(buf[25], 4); // day
        let start = u16::from_le_bytes([buf[26], buf[27]]);
        assert_eq!(start, 840); // 2:00 PM
        let dur = u16::from_le_bytes([buf[28], buf[29]]);
        assert_eq!(dur, 60);
    }

    #[test]
    fn event_pack_task_created() {
        let task = make_task();
        let event = Event::TaskCreated { revision: 1, task: task.clone() };

        let buf = pack_event(&event);
        assert_eq!(buf[0], msg::TASK_CREATED);
        assert_eq!(buf.len(), 1 + 8 + TASK_STRIDE); // type + rev + full task record
    }

    #[test]
    fn event_pack_task_deleted() {
        let event = Event::TaskDeleted {
            revision: 99,
            task_id: Uuid::from_bytes([0xBB; 16]),
        };

        let buf = pack_event(&event);
        assert_eq!(buf[0], msg::TASK_DELETED);
        assert_eq!(buf.len(), EVENT_HEADER);
        assert_eq!(&buf[9..25], &[0xBB; 16]);
    }

    #[test]
    fn unpack_move_task_command() {
        let task_id = Uuid::from_bytes([0xCC; 16]);
        let mut data = vec![msg::CMD_MOVE_TASK];
        data.extend_from_slice(task_id.as_bytes());
        data.push(2); // day
        data.extend_from_slice(&600u16.to_le_bytes()); // start_time (10:00)
        data.extend_from_slice(&45u16.to_le_bytes());  // duration

        let cmd = unpack_command(&data).unwrap();
        match cmd {
            Command::MoveTask { task_id: id, day, start_time, duration } => {
                assert_eq!(id, task_id);
                assert_eq!(day, 2);
                assert_eq!(start_time, 600);
                assert_eq!(duration, 45);
            }
            _ => panic!("expected MoveTask"),
        }
    }

    #[test]
    fn unpack_create_task_command() {
        let svc_id = Uuid::from_bytes([0x11; 16]);
        let mut data = vec![msg::CMD_CREATE_TASK];
        data.push(3); // priority = Urgent
        data.extend_from_slice(svc_id.as_bytes());
        data.extend_from_slice(&[0u8; 16]); // assigned_to = none
        data.extend_from_slice(b"Fix the pipeline");

        let cmd = unpack_command(&data).unwrap();
        match cmd {
            Command::CreateTask { title, service_id, priority, assigned_to } => {
                assert_eq!(title, "Fix the pipeline");
                assert_eq!(service_id, svc_id);
                assert_eq!(priority, Priority::Urgent);
                assert_eq!(assigned_to, None);
            }
            _ => panic!("expected CreateTask"),
        }
    }

    #[test]
    fn unpack_delete_task_command() {
        let task_id = Uuid::from_bytes([0xDD; 16]);
        let mut data = vec![msg::CMD_DELETE_TASK];
        data.extend_from_slice(task_id.as_bytes());

        let cmd = unpack_command(&data).unwrap();
        match cmd {
            Command::DeleteTask { task_id: id } => assert_eq!(id, task_id),
            _ => panic!("expected DeleteTask"),
        }
    }

    #[test]
    fn unpack_rejects_garbage() {
        assert_eq!(unpack_command(&[]).unwrap_err(), WireError::TooShort);
        assert_eq!(unpack_command(&[0x99]).unwrap_err(), WireError::UnknownMessage(0x99));
        assert_eq!(unpack_command(&[msg::CMD_MOVE_TASK, 0]).unwrap_err(), WireError::TooShort);
    }

    #[test]
    fn staged_task_day_is_0xff() {
        let task = Task {
            id: Uuid::nil(),
            title: "Staged".into(),
            status: TaskStatus::Staged,
            priority: Priority::Low,
            service_id: Uuid::nil(),
            created_by: Uuid::nil(),
            assigned_to: None,
            day: None,
            start_time: None,
            duration: None,
        };

        let mut buf = vec![0u8; TASK_STRIDE];
        pack_task(&mut buf, &task);
        assert_eq!(buf[18], 0xFF); // day = unscheduled marker
    }
}
