//! Sidecar renderer: converts World state into a flat RGBA image.
//!
//! tiny-skia handles all geometry. Text rendering is deferred (Phase 2).
//! Output: PNG bytes, sent over WebSocket as BackgroundImage (0x10).
//!
//! The browser sets this as CSS background-image. It never draws the world
//! itself. What the sidecar renders is exactly what the user sees.

use tiny_skia::{Color, Paint, Pixmap, Rect, Transform};
use crate::world::{Priority, World};

// ── Layout constants ─────────────────────────────────────────────────────────
// Mirror CONFIG in ironclad.js. Both sides must agree on these values.

const LEFT_GUTTER: f32 = 56.0;
const TOP_HEADER:  f32 = 32.0;
const DAY_WIDTH:   f32 = 180.0;
const HOUR_HEIGHT: f32 = 60.0;
const DAYS:        u32 = 7;
const START_HOUR:  u32 = 8;
const END_HOUR:    u32 = 18;
const SNAP_Y:      f32 = HOUR_HEIGHT / 4.0; // 15-min grid

// ── Palette ──────────────────────────────────────────────────────────────────
// RGBA tuples matching ironclad.js TYPE_COLORS and grid palette.

const BG:         (u8,u8,u8) = (14,  14,  18);   // #0e0e12
const GRID_HOUR:  (u8,u8,u8) = (37,  37,  48);   // #252530
const GRID_QHOUR: (u8,u8,u8) = (24,  24,  31);   // #18181f

// Task fill + 1px border, indexed by priority class (0=Low/Med, 1=High, 2=Urgent)
const TASK_FILL:   [(u8,u8,u8); 3] = [
    (30,  58,  95),  // #1e3a5f
    (26,  74,  58),  // #1a4a3a
    (74,  58,  26),  // #4a3a1a
];
const TASK_BORDER: [(u8,u8,u8); 3] = [
    (58,  123, 213), // #3a7bd5
    (45,  155, 122), // #2d9b7a
    (180, 136, 45),  // #b4882d
];

// ── Types ────────────────────────────────────────────────────────────────────

/// Which week is currently visible (week view).
pub struct ViewState {
    /// Epoch day (since 1970-01-01) of the Monday being shown.
    pub week_start: u16,
}

pub struct Renderer {
    width:  u32,
    height: u32,
}

impl Renderer {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Render the full world to PNG bytes.
    ///
    /// Call this after any world state change. Send the result to IRONCLAD
    /// as a BackgroundImage (0x10) message. Never call on cursor movement.
    pub fn render_world(&self, world: &World, view: &ViewState) -> Option<Vec<u8>> {
        let mut pixmap = Pixmap::new(self.width, self.height)?;

        // Background — fill entire canvas before drawing anything
        pixmap.fill(Color::from_rgba8(BG.0, BG.1, BG.2, 255));

        self.draw_grid(&mut pixmap);
        self.draw_tasks(&mut pixmap, world, view);

        pixmap.encode_png().ok()
    }

    // ── Grid ─────────────────────────────────────────────────────────────────

    fn draw_grid(&self, pixmap: &mut Pixmap) {
        let hours      = (END_HOUR - START_HOUR) as f32;
        let grid_right = LEFT_GUTTER + DAYS as f32 * DAY_WIDTH;
        let grid_h     = hours * HOUR_HEIGHT;

        let mut paint = Paint::default();
        paint.anti_alias = false;

        // Hour lines (full width)
        paint.set_color(Color::from_rgba8(GRID_HOUR.0, GRID_HOUR.1, GRID_HOUR.2, 255));
        for h in 0..=(END_HOUR - START_HOUR) {
            let y = TOP_HEADER + h as f32 * HOUR_HEIGHT;
            fill_rect(pixmap, &paint, LEFT_GUTTER, y, grid_right - LEFT_GUTTER, 1.0);
        }

        // 15-min sub-lines
        paint.set_color(Color::from_rgba8(GRID_QHOUR.0, GRID_QHOUR.1, GRID_QHOUR.2, 255));
        for h in 0..(END_HOUR - START_HOUR) {
            for q in 1..4_u32 {
                let y = TOP_HEADER + h as f32 * HOUR_HEIGHT + q as f32 * SNAP_Y;
                fill_rect(pixmap, &paint, LEFT_GUTTER, y, grid_right - LEFT_GUTTER, 1.0);
            }
        }

        // Day dividers (vertical)
        paint.set_color(Color::from_rgba8(GRID_HOUR.0, GRID_HOUR.1, GRID_HOUR.2, 255));
        for d in 0..=DAYS {
            let x = LEFT_GUTTER + d as f32 * DAY_WIDTH;
            fill_rect(pixmap, &paint, x, TOP_HEADER, 1.0, grid_h);
        }
    }

    // ── Tasks ─────────────────────────────────────────────────────────────────

    fn draw_tasks(&self, pixmap: &mut Pixmap, world: &World, view: &ViewState) {
        let week_start = view.week_start as i32;
        let pad = 10.0_f32;

        let mut paint = Paint::default();
        paint.anti_alias = false;

        for task in world.tasks.values() {
            // Skip staged tasks (no date/time) and completed tasks
            let (date, start_time, duration) = match (task.date, task.start_time, task.duration) {
                (Some(d), Some(s), Some(dur)) => (d as i32, s, dur),
                _ => continue,
            };

            // Skip tasks not in the current week
            let col = date - week_start;
            if col < 0 || col >= DAYS as i32 { continue; }

            let x = LEFT_GUTTER + col as f32 * DAY_WIDTH + pad;
            let y = TOP_HEADER + (start_time as f32 / 60.0 - START_HOUR as f32) * HOUR_HEIGHT;
            let w = DAY_WIDTH - pad * 2.0;
            let h = (duration as f32 / 60.0) * HOUR_HEIGHT;

            if w < 1.0 || h < 2.0 { continue; }

            let ti = match task.priority {
                Priority::Urgent           => 2,
                Priority::High             => 1,
                Priority::Low |
                Priority::Medium           => 0,
            };

            // Fill
            let f = TASK_FILL[ti];
            paint.set_color(Color::from_rgba8(f.0, f.1, f.2, 255));
            fill_rect(pixmap, &paint, x, y, w, h);

            // 1px border (four thin rects — no path needed)
            let b = TASK_BORDER[ti];
            paint.set_color(Color::from_rgba8(b.0, b.1, b.2, 255));
            fill_rect(pixmap, &paint, x,         y,         w,   1.0); // top
            fill_rect(pixmap, &paint, x,         y + h - 1.0, w, 1.0); // bottom
            fill_rect(pixmap, &paint, x,         y,         1.0,   h); // left
            fill_rect(pixmap, &paint, x + w - 1.0, y,       1.0,   h); // right
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Fill a rectangle. Silently skips invalid rects (zero size, non-finite coords).
#[inline]
fn fill_rect(pixmap: &mut Pixmap, paint: &Paint, x: f32, y: f32, w: f32, h: f32) {
    if let Some(rect) = Rect::from_xywh(x, y, w, h) {
        pixmap.fill_rect(rect, paint, Transform::identity(), None);
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Command, World};
    use uuid::Uuid;

    fn monday_this_week() -> u16 {
        let days = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() / 86400;
        let dow = ((days + 3) % 7) as u16; // (epoch_day + 3) % 7 → 0=Mon
        (days as u16).saturating_sub(dow)
    }

    #[test]
    fn renders_empty_world_to_png() {
        let world = World::new();
        let renderer = Renderer::new(1316, 632);
        let view = ViewState { week_start: monday_this_week() };

        let png = renderer.render_world(&world, &view)
            .expect("render_world returned None — Pixmap allocation failed");

        assert!(!png.is_empty());
        assert_eq!(&png[1..4], b"PNG", "Output is not a valid PNG");

        std::fs::create_dir_all(".tmp").unwrap();
        std::fs::write(".tmp/test_render_empty.png", &png).unwrap();
        println!("Grid render: {} bytes → .tmp/test_render_empty.png", png.len());
    }

    #[test]
    fn renders_world_with_tasks() {
        use crate::world::Service;

        let mut world = World::new();
        let user_id    = Uuid::new_v4();
        let service_id = Uuid::new_v4();
        let monday     = monday_this_week();

        // Service must exist in world before CreateTask will accept it
        world.services.insert(service_id, Service { id: service_id, name: "Test".into() });

        // Seed tasks across the week at different priorities
        for (col, hour, priority) in [
            (0_u16, 9_u16,  Priority::Medium),
            (1,     11,     Priority::High),
            (2,     14,     Priority::Urgent),
            (4,     10,     Priority::Low),
            (6,     9,      Priority::Medium),
        ] {
            let _ = world.apply(
                Command::CreateTask {
                    title:       format!("Task {} {}:00", col, hour),
                    service_id,
                    priority,
                    assigned_to: None,
                    date:        Some(monday + col),
                    start_time:  Some(hour * 60),
                    duration:    Some(60),
                },
                user_id,
            );
        }

        let renderer = Renderer::new(1316, 632);
        let view = ViewState { week_start: monday };

        let png = renderer.render_world(&world, &view)
            .expect("render_world returned None");

        assert!(!png.is_empty());
        assert_eq!(&png[1..4], b"PNG");

        std::fs::create_dir_all(".tmp").unwrap();
        std::fs::write(".tmp/test_render_tasks.png", &png).unwrap();
        println!("Task render: {} bytes → .tmp/test_render_tasks.png", png.len());
    }
}
