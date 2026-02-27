//! Binary wire protocol — hop 1 (browser ↔ sidecar).
//!
//! Fixed-stride packed records, readable by JS DataView at known offsets.
//! All multi-byte integers are little-endian.
//!
//! Hop 2 (sidecar ↔ EC2) is stubbed — dead code until EC2 is ready.

use crate::world::{Command, Event, Priority, Service, Task, World};
use uuid::Uuid;

// ── Message types ─────────────────────────────────────────────

pub mod msg {
    // Sidecar → Browser
    pub const BACKGROUND_IMAGE: u8  = 0x10;
    pub const CANDIDATE_LIST: u8    = 0x11;
    pub const SNAP_HINT: u8         = 0x18;
    pub const CURSOR_STYLE: u8      = 0x19;
    pub const OVERLAY_COMMANDS: u8  = 0x1A;

    // Browser → Sidecar
    pub const VIEWPORT_SIZE: u8     = 0x20;
    pub const SET_VIEW: u8          = 0x21;
    pub const VISIBILITY_CHANGE: u8 = 0x22;
    pub const CURSOR_MOVE: u8       = 0x23;
    pub const MODE_CHANGE: u8       = 0x24;
    pub const CLICK_AT: u8          = 0x25;
    pub const NAVIGATE_WINDOW: u8   = 0x26;
    pub const CREATE_AT: u8         = 0x27;
    pub const DROP_TASK: u8         = 0x28;
    pub const RESIZE_TASK: u8       = 0x29;
    pub const UNSCHEDULE_TASK: u8   = 0x2A;
    pub const COMPLETE_TASK: u8     = 0x2B;
    pub const DELETE_TASK: u8       = 0x2C;
    pub const UPDATE_TITLE: u8      = 0x2D;
    pub const SET_PRIORITY: u8      = 0x2E;
}

// ── Image format ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ImageFormat {
    Png = 0,
    Jpeg = 1,
    Rgba = 2,
}

// ── Overlay command types ─────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OverlayCmdType {
    Clear = 0,
    Arc = 1,
    Segment = 2,
}

/// An overlay draw command. Produced by spatial.rs, packed for the wire.
#[derive(Debug, Clone)]
pub enum OverlayCommand {
    Clear,
    Segment {
        x1: f32, y1: f32,
        x2: f32, y2: f32,
    },
    Arc {
        cx: f32, cy: f32,
        r: f32,
        start_angle: f32,
        sweep_angle: f32,
    },
}

// ── Candidate list entry ──────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Candidate {
    pub task_id: Uuid,
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub color: u32,
    pub sdf_dist: i16,
    pub title: String,
}

// ── Client messages ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ViewMode {
    Week = 0,
    Month = 1,
    Day = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum InteractionMode {
    Idle = 0,
    Drag = 1,
    Typing = 2,
}

#[derive(Debug, Clone)]
pub enum ClientMsg {
    ViewportSize { width: u16, height: u16 },
    SetView { mode: ViewMode },
    VisibilityChange { visible: bool, last_seen_rev: u64 },
    CursorMove { x: u16, y: u16 },
    ModeChange { mode: InteractionMode },
    ClickAt { x: u16, y: u16 },
    NavigateWindow { delta: i8 },
    CreateAt { x: u16, y: u16 },
    DropTask { task_id: u16, x: u16, y: u16 },
    ResizeTask { task_id: u16, bottom_y: u16 },
    UnscheduleTask { task_id: u16 },
    CompleteTask { task_id: u16 },
    DeleteTask { task_id: u16 },
    UpdateTitle { task_id: u16, title: String },
    SetPriority { task_id: u16, priority: u8 },
}

// ── Layout constants (hop 2 — task record stride) ─────────────

pub const TASK_STRIDE: usize = 192;
pub const TITLE_MAX: usize = 128;
pub const SERVICE_STRIDE: usize = 80;
pub const SERVICE_NAME_MAX: usize = 64;

// ── Packing: Sidecar → Browser ───────────────────────────────

/// Pack a background image message (0x10).
pub fn pack_background_image(revision: u64, format: ImageFormat, width: u16, height: u16, image_bytes: &[u8]) -> Vec<u8> {
    let header_len = 1 + 8 + 1 + 2 + 2; // type + rev + format + w + h
    let mut buf = Vec::with_capacity(header_len + image_bytes.len());
    buf.push(msg::BACKGROUND_IMAGE);
    buf.extend_from_slice(&revision.to_le_bytes());
    buf.push(format as u8);
    buf.extend_from_slice(&width.to_le_bytes());
    buf.extend_from_slice(&height.to_le_bytes());
    buf.extend_from_slice(image_bytes);
    buf
}

/// Pack overlay commands (0x1A).
/// Wire format: [type:u8][count:u8] then per command:
///
///   Segment: [cmd:u8=2][x1:u16][y1:u16][x2:u16][y2:u16][color:u32]          = 13 bytes
///   Arc:     [cmd:u8=1][cx:f32][cy:f32][r:f32][start:f32][sweep:f32][color:u32] = 25 bytes
///   Clear:   [cmd:u8=0][0×12 padding]                                         = 13 bytes
///
/// Arc sends full geometry so the browser can reconstruct the SVG arc path.
pub fn pack_overlay_commands(commands: &[OverlayCommand], color: u32) -> Vec<u8> {
    let count = commands.len().min(255) as u8;
    let mut buf = Vec::with_capacity(2 + count as usize * 25); // 25 = worst case (arc)
    buf.push(msg::OVERLAY_COMMANDS);
    buf.push(count);

    for cmd in commands.iter().take(count as usize) {
        match cmd {
            OverlayCommand::Clear => {
                buf.push(OverlayCmdType::Clear as u8);
                buf.extend_from_slice(&[0u8; 12]); // padding to 13 bytes
            }
            OverlayCommand::Segment { x1, y1, x2, y2 } => {
                buf.push(OverlayCmdType::Segment as u8);
                buf.extend_from_slice(&(*x1 as u16).to_le_bytes());
                buf.extend_from_slice(&(*y1 as u16).to_le_bytes());
                buf.extend_from_slice(&(*x2 as u16).to_le_bytes());
                buf.extend_from_slice(&(*y2 as u16).to_le_bytes());
                buf.extend_from_slice(&color.to_le_bytes());
            }
            OverlayCommand::Arc { cx, cy, r, start_angle, sweep_angle } => {
                buf.push(OverlayCmdType::Arc as u8);
                buf.extend_from_slice(&cx.to_le_bytes());
                buf.extend_from_slice(&cy.to_le_bytes());
                buf.extend_from_slice(&r.to_le_bytes());
                buf.extend_from_slice(&start_angle.to_le_bytes());
                buf.extend_from_slice(&sweep_angle.to_le_bytes());
                buf.extend_from_slice(&color.to_le_bytes());
            }
        }
    }
    buf
}

/// Pack a candidate list (0x11).
pub fn pack_candidate_list(candidates: &[Candidate]) -> Vec<u8> {
    let count = candidates.len().min(255) as u8;
    // type + count + per candidate: task_id(2) + x(2) + y(2) + w(2) + h(2) + color(4) + sdf_dist(2) + title_len(1) + title
    let mut buf = Vec::with_capacity(2 + count as usize * 32);
    buf.push(msg::CANDIDATE_LIST);
    buf.push(count);

    for c in candidates.iter().take(count as usize) {
        // task_id as u16 (wire uses short IDs, not full UUIDs — mapped in game.rs)
        // For now we use the first 2 bytes of the UUID as a short ID placeholder
        let short_id = u16::from_le_bytes([c.task_id.as_bytes()[0], c.task_id.as_bytes()[1]]);
        buf.extend_from_slice(&short_id.to_le_bytes());
        buf.extend_from_slice(&c.x.to_le_bytes());
        buf.extend_from_slice(&c.y.to_le_bytes());
        buf.extend_from_slice(&c.w.to_le_bytes());
        buf.extend_from_slice(&c.h.to_le_bytes());
        buf.extend_from_slice(&c.color.to_le_bytes());
        buf.extend_from_slice(&c.sdf_dist.to_le_bytes());
        let title_bytes = c.title.as_bytes();
        let title_len = title_bytes.len().min(255) as u8;
        buf.push(title_len);
        buf.extend_from_slice(&title_bytes[..title_len as usize]);
    }
    buf
}

/// Pack a snap hint (0x18).
pub fn pack_snap_hint(x: u16, y: u16) -> Vec<u8> {
    let mut buf = Vec::with_capacity(5);
    buf.push(msg::SNAP_HINT);
    buf.extend_from_slice(&x.to_le_bytes());
    buf.extend_from_slice(&y.to_le_bytes());
    buf
}

/// Pack a cursor style (0x19).
pub fn pack_cursor_style(style: u8) -> Vec<u8> {
    vec![msg::CURSOR_STYLE, style]
}

// ── Unpacking: Browser → Sidecar ─────────────────────────────

/// Unpack a binary message from the browser.
pub fn unpack_client_msg(data: &[u8]) -> Result<ClientMsg, WireError> {
    if data.is_empty() {
        return Err(WireError::TooShort);
    }

    match data[0] {
        msg::VIEWPORT_SIZE => {
            ensure_len(data, 5)?;
            Ok(ClientMsg::ViewportSize {
                width: u16_at(data, 1),
                height: u16_at(data, 3),
            })
        }

        msg::SET_VIEW => {
            ensure_len(data, 2)?;
            let mode = match data[1] {
                0 => ViewMode::Week,
                1 => ViewMode::Month,
                2 => ViewMode::Day,
                _ => return Err(WireError::InvalidField("view mode")),
            };
            Ok(ClientMsg::SetView { mode })
        }

        msg::VISIBILITY_CHANGE => {
            ensure_len(data, 10)?;
            Ok(ClientMsg::VisibilityChange {
                visible: data[1] != 0,
                last_seen_rev: u64::from_le_bytes(data[2..10].try_into().unwrap()),
            })
        }

        msg::CURSOR_MOVE => {
            ensure_len(data, 5)?;
            Ok(ClientMsg::CursorMove {
                x: u16_at(data, 1),
                y: u16_at(data, 3),
            })
        }

        msg::MODE_CHANGE => {
            ensure_len(data, 2)?;
            let mode = match data[1] {
                0 => InteractionMode::Idle,
                1 => InteractionMode::Drag,
                2 => InteractionMode::Typing,
                _ => return Err(WireError::InvalidField("interaction mode")),
            };
            Ok(ClientMsg::ModeChange { mode })
        }

        msg::CLICK_AT => {
            ensure_len(data, 5)?;
            Ok(ClientMsg::ClickAt {
                x: u16_at(data, 1),
                y: u16_at(data, 3),
            })
        }

        msg::NAVIGATE_WINDOW => {
            ensure_len(data, 2)?;
            Ok(ClientMsg::NavigateWindow {
                delta: data[1] as i8,
            })
        }

        msg::CREATE_AT => {
            ensure_len(data, 5)?;
            Ok(ClientMsg::CreateAt {
                x: u16_at(data, 1),
                y: u16_at(data, 3),
            })
        }

        msg::DROP_TASK => {
            ensure_len(data, 7)?;
            Ok(ClientMsg::DropTask {
                task_id: u16_at(data, 1),
                x: u16_at(data, 3),
                y: u16_at(data, 5),
            })
        }

        msg::RESIZE_TASK => {
            ensure_len(data, 5)?;
            Ok(ClientMsg::ResizeTask {
                task_id: u16_at(data, 1),
                bottom_y: u16_at(data, 3),
            })
        }

        msg::UNSCHEDULE_TASK => {
            ensure_len(data, 3)?;
            Ok(ClientMsg::UnscheduleTask {
                task_id: u16_at(data, 1),
            })
        }

        msg::COMPLETE_TASK => {
            ensure_len(data, 3)?;
            Ok(ClientMsg::CompleteTask {
                task_id: u16_at(data, 1),
            })
        }

        msg::DELETE_TASK => {
            ensure_len(data, 3)?;
            Ok(ClientMsg::DeleteTask {
                task_id: u16_at(data, 1),
            })
        }

        msg::UPDATE_TITLE => {
            ensure_len(data, 3)?;
            let task_id = u16_at(data, 1);
            let title = std::str::from_utf8(&data[3..])
                .map_err(|_| WireError::InvalidUtf8)?
                .to_string();
            Ok(ClientMsg::UpdateTitle { task_id, title })
        }

        msg::SET_PRIORITY => {
            ensure_len(data, 4)?;
            Ok(ClientMsg::SetPriority {
                task_id: u16_at(data, 1),
                priority: data[3],
            })
        }

        other => Err(WireError::UnknownMessage(other)),
    }
}

// ── Hop 2 packing (sidecar ↔ EC2) — stubs ────────────────────

/// Pack a full world snapshot for hop 2 (EC2 protocol).
/// Uses the old fixed-stride format from the archive.
#[allow(dead_code)]
pub fn pack_snapshot(world: &World) -> Vec<u8> {
    let task_count = world.tasks.len();
    let service_count = world.services.len();
    let header = 17; // type(1) + rev(8) + task_count(4) + svc_count(4)
    let size = header + task_count * TASK_STRIDE + service_count * SERVICE_STRIDE;

    let mut buf = vec![0u8; size];
    buf[0] = 0x01; // WorldSnapshot
    buf[1..9].copy_from_slice(&world.revision.to_le_bytes());
    buf[9..13].copy_from_slice(&(task_count as u32).to_le_bytes());
    buf[13..17].copy_from_slice(&(service_count as u32).to_le_bytes());

    let mut offset = header;
    for task in world.tasks.values() {
        pack_task_record(&mut buf[offset..offset + TASK_STRIDE], task);
        offset += TASK_STRIDE;
    }
    for service in world.services.values() {
        pack_service_record(&mut buf[offset..offset + SERVICE_STRIDE], service);
        offset += SERVICE_STRIDE;
    }
    buf
}

/// Pack a single event for hop 2 broadcast.
#[allow(dead_code)]
pub fn pack_event(event: &Event) -> Vec<u8> {
    match event {
        Event::TaskCreated { revision, task } => {
            let mut buf = vec![0u8; 1 + 8 + TASK_STRIDE];
            buf[0] = 0x02;
            buf[1..9].copy_from_slice(&revision.to_le_bytes());
            pack_task_record(&mut buf[9..9 + TASK_STRIDE], task);
            buf
        }
        Event::TaskScheduled { revision, task_id, start, duration } => {
            let mut buf = vec![0u8; 25 + 6];
            buf[0] = 0x03;
            buf[1..9].copy_from_slice(&revision.to_le_bytes());
            buf[9..25].copy_from_slice(task_id.as_bytes());
            buf[25..29].copy_from_slice(&start.to_le_bytes());
            buf[29..31].copy_from_slice(&duration.to_le_bytes());
            buf
        }
        Event::TaskMoved { revision, task_id, start, duration } => {
            let mut buf = vec![0u8; 25 + 6];
            buf[0] = 0x03; // same as scheduled for hop2
            buf[1..9].copy_from_slice(&revision.to_le_bytes());
            buf[9..25].copy_from_slice(task_id.as_bytes());
            buf[25..29].copy_from_slice(&start.to_le_bytes());
            buf[29..31].copy_from_slice(&duration.to_le_bytes());
            buf
        }
        Event::TaskUnscheduled { revision, task_id } => {
            let mut buf = vec![0u8; 25];
            buf[0] = 0x04;
            buf[1..9].copy_from_slice(&revision.to_le_bytes());
            buf[9..25].copy_from_slice(task_id.as_bytes());
            buf
        }
        Event::TaskCompleted { revision, task_id } => {
            let mut buf = vec![0u8; 25];
            buf[0] = 0x05;
            buf[1..9].copy_from_slice(&revision.to_le_bytes());
            buf[9..25].copy_from_slice(task_id.as_bytes());
            buf
        }
        Event::TaskDeleted { revision, task_id } => {
            let mut buf = vec![0u8; 25];
            buf[0] = 0x06;
            buf[1..9].copy_from_slice(&revision.to_le_bytes());
            buf[9..25].copy_from_slice(task_id.as_bytes());
            buf
        }
    }
}

// ── Task/service record packing (hop 2 format) ───────────────

fn pack_task_record(buf: &mut [u8], task: &Task) {
    buf[0..16].copy_from_slice(task.id.as_bytes());
    buf[16] = task.status as u8;
    buf[17] = task.priority as u8;
    buf[18..22].copy_from_slice(&task.start.unwrap_or(0xFFFF_FFFF).to_le_bytes());
    buf[22..24].copy_from_slice(&task.duration.unwrap_or(0).to_le_bytes());
    buf[24..40].copy_from_slice(task.service_id.as_bytes());
    buf[40..56].copy_from_slice(
        task.assigned_to.unwrap_or(Uuid::nil()).as_bytes(),
    );
    let title_bytes = task.title.as_bytes();
    let len = title_bytes.len().min(TITLE_MAX);
    buf[56..56 + len].copy_from_slice(&title_bytes[..len]);
    buf[184] = task.artifact_count;
}

fn pack_service_record(buf: &mut [u8], service: &Service) {
    buf[0..16].copy_from_slice(service.id.as_bytes());
    let name_bytes = service.name.as_bytes();
    let len = name_bytes.len().min(SERVICE_NAME_MAX);
    buf[16..16 + len].copy_from_slice(&name_bytes[..len]);
}

// ── Helpers ───────────────────────────────────────────────────

fn ensure_len(data: &[u8], min: usize) -> Result<(), WireError> {
    if data.len() < min {
        Err(WireError::TooShort)
    } else {
        Ok(())
    }
}

fn u16_at(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn string_from_bytes(b: &[u8]) -> Result<String, WireError> {
    let end = b.iter().rposition(|&c| c != 0).map_or(0, |i| i + 1);
    std::str::from_utf8(&b[..end])
        .map(|s| s.to_string())
        .map_err(|_| WireError::InvalidUtf8)
}

// ── Errors ────────────────────────────────────────────────────

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

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::TaskStatus;

    #[test]
    fn background_image_header() {
        let img = vec![0xAA; 100];
        let buf = pack_background_image(42, ImageFormat::Png, 1316, 632, &img);

        assert_eq!(buf[0], msg::BACKGROUND_IMAGE);
        let rev = u64::from_le_bytes(buf[1..9].try_into().unwrap());
        assert_eq!(rev, 42);
        assert_eq!(buf[9], 0); // PNG
        assert_eq!(u16_at(&buf, 10), 1316);
        assert_eq!(u16_at(&buf, 12), 632);
        assert_eq!(&buf[14..], &img[..]);
    }

    #[test]
    fn overlay_commands_pack() {
        let cmds = vec![
            OverlayCommand::Segment { x1: 10.0, y1: 20.0, x2: 30.0, y2: 40.0 },
            OverlayCommand::Clear,
        ];
        let buf = pack_overlay_commands(&cmds, 0x00FF00FF);

        assert_eq!(buf[0], msg::OVERLAY_COMMANDS);
        assert_eq!(buf[1], 2); // count

        // First command: Segment
        assert_eq!(buf[2], OverlayCmdType::Segment as u8);
        assert_eq!(u16_at(&buf, 3), 10);  // x1
        assert_eq!(u16_at(&buf, 5), 20);  // y1
        assert_eq!(u16_at(&buf, 7), 30);  // x2
        assert_eq!(u16_at(&buf, 9), 40);  // y2

        // Second command: Clear
        assert_eq!(buf[15], OverlayCmdType::Clear as u8);
    }

    #[test]
    fn unpack_cursor_move() {
        let data = [msg::CURSOR_MOVE, 0x00, 0x02, 0x58, 0x01]; // x=512, y=344
        let msg = unpack_client_msg(&data).unwrap();
        match msg {
            ClientMsg::CursorMove { x, y } => {
                assert_eq!(x, 512);
                assert_eq!(y, 344);
            }
            _ => panic!("expected CursorMove"),
        }
    }

    #[test]
    fn unpack_viewport_size() {
        let mut data = vec![msg::VIEWPORT_SIZE];
        data.extend_from_slice(&1316u16.to_le_bytes());
        data.extend_from_slice(&632u16.to_le_bytes());
        let msg = unpack_client_msg(&data).unwrap();
        match msg {
            ClientMsg::ViewportSize { width, height } => {
                assert_eq!(width, 1316);
                assert_eq!(height, 632);
            }
            _ => panic!("expected ViewportSize"),
        }
    }

    #[test]
    fn unpack_set_view() {
        let data = [msg::SET_VIEW, 2]; // Day
        let msg = unpack_client_msg(&data).unwrap();
        match msg {
            ClientMsg::SetView { mode } => assert_eq!(mode, ViewMode::Day),
            _ => panic!("expected SetView"),
        }
    }

    #[test]
    fn unpack_click_at() {
        let mut data = vec![msg::CLICK_AT];
        data.extend_from_slice(&100u16.to_le_bytes());
        data.extend_from_slice(&200u16.to_le_bytes());
        let msg = unpack_client_msg(&data).unwrap();
        match msg {
            ClientMsg::ClickAt { x, y } => {
                assert_eq!(x, 100);
                assert_eq!(y, 200);
            }
            _ => panic!("expected ClickAt"),
        }
    }

    #[test]
    fn unpack_navigate_window() {
        let data = [msg::NAVIGATE_WINDOW, 0xFF]; // -1 as i8
        let msg = unpack_client_msg(&data).unwrap();
        match msg {
            ClientMsg::NavigateWindow { delta } => assert_eq!(delta, -1),
            _ => panic!("expected NavigateWindow"),
        }
    }

    #[test]
    fn unpack_create_at() {
        let mut data = vec![msg::CREATE_AT];
        data.extend_from_slice(&300u16.to_le_bytes());
        data.extend_from_slice(&150u16.to_le_bytes());
        let msg = unpack_client_msg(&data).unwrap();
        match msg {
            ClientMsg::CreateAt { x, y } => {
                assert_eq!(x, 300);
                assert_eq!(y, 150);
            }
            _ => panic!("expected CreateAt"),
        }
    }

    #[test]
    fn unpack_update_title() {
        let mut data = vec![msg::UPDATE_TITLE];
        data.extend_from_slice(&7u16.to_le_bytes());
        data.extend_from_slice(b"New title");
        let msg = unpack_client_msg(&data).unwrap();
        match msg {
            ClientMsg::UpdateTitle { task_id, title } => {
                assert_eq!(task_id, 7);
                assert_eq!(title, "New title");
            }
            _ => panic!("expected UpdateTitle"),
        }
    }

    #[test]
    fn unpack_delete_task() {
        let mut data = vec![msg::DELETE_TASK];
        data.extend_from_slice(&42u16.to_le_bytes());
        let msg = unpack_client_msg(&data).unwrap();
        match msg {
            ClientMsg::DeleteTask { task_id } => assert_eq!(task_id, 42),
            _ => panic!("expected DeleteTask"),
        }
    }

    #[test]
    fn unpack_rejects_garbage() {
        assert_eq!(unpack_client_msg(&[]).unwrap_err(), WireError::TooShort);
        assert_eq!(unpack_client_msg(&[0x99]).unwrap_err(), WireError::UnknownMessage(0x99));
        assert_eq!(unpack_client_msg(&[msg::CURSOR_MOVE, 0]).unwrap_err(), WireError::TooShort);
    }

    #[test]
    fn hop2_snapshot_layout() {
        let mut world = World::new();
        let task = Task {
            id: Uuid::from_bytes([1; 16]),
            title: "Deploy the widget".into(),
            status: TaskStatus::Scheduled,
            priority: Priority::High,
            service_id: Uuid::from_bytes([2; 16]),
            created_by: Uuid::from_bytes([3; 16]),
            assigned_to: Some(Uuid::from_bytes([4; 16])),
            start: Some(29_513_340),
            duration: Some(90),
            artifact_count: 2,
        };
        let service = Service {
            id: Uuid::from_bytes([5; 16]),
            name: "Billing Portal".into(),
        };
        world.tasks.insert(task.id, task);
        world.services.insert(service.id, service);
        world.revision = 42;

        let buf = pack_snapshot(&world);

        assert_eq!(buf[0], 0x01);
        let rev = u64::from_le_bytes(buf[1..9].try_into().unwrap());
        assert_eq!(rev, 42);
        let task_count = u32::from_le_bytes(buf[9..13].try_into().unwrap());
        assert_eq!(task_count, 1);

        // Task record at offset 17
        let t = &buf[17..17 + TASK_STRIDE];
        assert_eq!(&t[0..16], &[1u8; 16]); // id
        assert_eq!(t[16], TaskStatus::Scheduled as u8);
        assert_eq!(t[17], Priority::High as u8);
        assert_eq!(t[184], 2); // artifact_count
    }

    #[test]
    fn staged_task_sentinel() {
        let task = Task {
            id: Uuid::nil(),
            title: "Staged".into(),
            status: TaskStatus::Staged,
            priority: Priority::Low,
            service_id: Uuid::nil(),
            created_by: Uuid::nil(),
            assigned_to: None,
            start: None,
            duration: None,
            artifact_count: 0,
        };

        let mut buf = vec![0u8; TASK_STRIDE];
        pack_task_record(&mut buf, &task);
        let start = u32::from_le_bytes([buf[18], buf[19], buf[20], buf[21]]);
        assert_eq!(start, 0xFFFF_FFFF);
    }
}
