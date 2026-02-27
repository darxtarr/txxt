//! Sidecar renderer: converts World state into a flat image + shape metadata.
//!
//! tiny-skia handles all geometry. Output:
//! 1. Image bytes (PNG/JPEG/RGBA) — sent as BackgroundImage (0x10)
//! 2. Vec<RenderedShape> — pixel-space bounding boxes for the spatial module
//!
//! Chrome (grid + headers) is cached and only rebuilt on viewport/view change.
//! Task layer is stamped on a clone of the chrome cache on every mutation.

use tiny_skia::{Color, Paint, Pixmap, Rect, Transform};
use crate::wire::ImageFormat;
use crate::world::{Priority, World};
use uuid::Uuid;

// ── Layout constants ─────────────────────────────────────────────

const LEFT_GUTTER: f32 = 56.0;
const TOP_HEADER:  f32 = 32.0;
const DAY_WIDTH:   f32 = 180.0;
const HOUR_HEIGHT: f32 = 60.0;
const DAYS:        u32 = 7;
const START_HOUR:  u32 = 8;
const END_HOUR:    u32 = 18;
const TASK_PAD:    f32 = 10.0;

// ── Palette ──────────────────────────────────────────────────────

const BG:         (u8,u8,u8) = (14,  14,  18);
const GRID_HOUR:  (u8,u8,u8) = (37,  37,  48);
const GRID_QHOUR: (u8,u8,u8) = (24,  24,  31);

const TASK_FILL: [(u8,u8,u8); 3] = [
    (30,  58,  95),   // Low/Medium
    (26,  74,  58),   // High
    (74,  58,  26),   // Urgent
];
const TASK_BORDER: [(u8,u8,u8); 3] = [
    (58,  123, 213),
    (45,  155, 122),
    (180, 136, 45),
];

// ── Types ────────────────────────────────────────────────────────

/// Shape kind for SDF dispatch in spatial.rs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKind {
    Rect,
    RoundedRect,
}

/// Pixel-space bounding box of a rendered shape. Output of the renderer,
/// consumed by spatial.rs for SDF evaluation and overlay generation.
#[derive(Debug, Clone)]
pub struct RenderedShape {
    pub task_id: Uuid,
    pub kind: ShapeKind,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub corner_radius: f32,
    pub color: u32,
}

/// The visible time window. All times are minutes since Unix epoch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewState {
    pub window_start: u32,
    pub window_end: u32,
}

/// Key for chrome cache invalidation.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ChromeCacheKey {
    width: u32,
    height: u32,
    window_start: u32,
    window_end: u32,
}

pub struct Renderer {
    pub width: u32,
    pub height: u32,
    chrome_cache: Option<Pixmap>,
    chrome_cache_key: Option<ChromeCacheKey>,
}

/// Output of render_world: image bytes + shape metadata for spatial queries.
pub struct RenderOutput {
    pub image_bytes: Vec<u8>,
    pub shapes: Vec<RenderedShape>,
}

impl Renderer {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            chrome_cache: None,
            chrome_cache_key: None,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            self.chrome_cache = None;
            self.chrome_cache_key = None;
        }
    }

    /// Render the world. Returns image bytes + shape positions.
    pub fn render_world(
        &mut self,
        world: &World,
        view: &ViewState,
        format: ImageFormat,
    ) -> Option<RenderOutput> {
        let key = ChromeCacheKey {
            width: self.width,
            height: self.height,
            window_start: view.window_start,
            window_end: view.window_end,
        };

        // Rebuild chrome cache if needed
        if self.chrome_cache_key.as_ref() != Some(&key) {
            let mut chrome = Pixmap::new(self.width, self.height)?;
            chrome.fill(Color::from_rgba8(BG.0, BG.1, BG.2, 255));
            Self::draw_grid(&mut chrome);
            self.chrome_cache = Some(chrome);
            self.chrome_cache_key = Some(key);
        }

        // Clone chrome, stamp tasks on top
        let mut pixmap = self.chrome_cache.as_ref().unwrap().clone();
        let shapes = self.draw_tasks(&mut pixmap, world, view);

        let image_bytes = match format {
            ImageFormat::Png => pixmap.encode_png().ok()?,
            ImageFormat::Jpeg => {
                // Stub — would use the `image` crate with jpeg feature
                pixmap.encode_png().ok()?
            }
            ImageFormat::Rgba => pixmap.data().to_vec(),
        };

        Some(RenderOutput { image_bytes, shapes })
    }

    /// Check if the chrome cache would be a hit for the given view.
    pub fn chrome_cache_hit(&self, view: &ViewState) -> bool {
        self.chrome_cache_key.as_ref() == Some(&ChromeCacheKey {
            width: self.width,
            height: self.height,
            window_start: view.window_start,
            window_end: view.window_end,
        })
    }

    // ── Grid (chrome layer) ──────────────────────────────────────

    fn draw_grid(pixmap: &mut Pixmap) {
        let hours      = (END_HOUR - START_HOUR) as f32;
        let grid_right = LEFT_GUTTER + DAYS as f32 * DAY_WIDTH;
        let grid_h     = hours * HOUR_HEIGHT;

        let mut paint = Paint::default();
        paint.anti_alias = false;

        // Hour lines
        paint.set_color(Color::from_rgba8(GRID_HOUR.0, GRID_HOUR.1, GRID_HOUR.2, 255));
        for h in 0..=(END_HOUR - START_HOUR) {
            let y = TOP_HEADER + h as f32 * HOUR_HEIGHT;
            fill_rect(pixmap, &paint, LEFT_GUTTER, y, grid_right - LEFT_GUTTER, 1.0);
        }

        // 15-min sub-lines
        paint.set_color(Color::from_rgba8(GRID_QHOUR.0, GRID_QHOUR.1, GRID_QHOUR.2, 255));
        for h in 0..(END_HOUR - START_HOUR) {
            for q in 1..4_u32 {
                let y = TOP_HEADER + h as f32 * HOUR_HEIGHT + q as f32 * (HOUR_HEIGHT / 4.0);
                fill_rect(pixmap, &paint, LEFT_GUTTER, y, grid_right - LEFT_GUTTER, 1.0);
            }
        }

        // Day dividers
        paint.set_color(Color::from_rgba8(GRID_HOUR.0, GRID_HOUR.1, GRID_HOUR.2, 255));
        for d in 0..=DAYS {
            let x = LEFT_GUTTER + d as f32 * DAY_WIDTH;
            fill_rect(pixmap, &paint, x, TOP_HEADER, 1.0, grid_h);
        }
    }

    // ── Tasks ────────────────────────────────────────────────────

    fn draw_tasks(&self, pixmap: &mut Pixmap, world: &World, view: &ViewState) -> Vec<RenderedShape> {
        let window_day_start = view.window_start / 1440;
        let mut shapes = Vec::new();
        let mut paint = Paint::default();
        paint.anti_alias = false;

        for task in world.tasks.values() {
            let (start, duration) = match (task.start, task.duration) {
                (Some(s), Some(dur)) => (s, dur),
                _ => continue,
            };

            if start < view.window_start || start >= view.window_end { continue; }

            let epoch_day = start / 1440;
            let col = epoch_day - window_day_start;
            if col >= DAYS { continue; }

            let time_of_day = start % 1440;

            let x = LEFT_GUTTER + col as f32 * DAY_WIDTH + TASK_PAD;
            let y = TOP_HEADER + (time_of_day as f32 / 60.0 - START_HOUR as f32) * HOUR_HEIGHT;
            let w = DAY_WIDTH - TASK_PAD * 2.0;
            let h = (duration as f32 / 60.0) * HOUR_HEIGHT;

            if w < 1.0 || h < 2.0 { continue; }

            let ti = match task.priority {
                Priority::Urgent       => 2,
                Priority::High         => 1,
                Priority::Low | Priority::Medium => 0,
            };

            // Fill
            let f = TASK_FILL[ti];
            paint.set_color(Color::from_rgba8(f.0, f.1, f.2, 255));
            fill_rect(pixmap, &paint, x, y, w, h);

            // 1px border
            let b = TASK_BORDER[ti];
            paint.set_color(Color::from_rgba8(b.0, b.1, b.2, 255));
            fill_rect(pixmap, &paint, x,         y,         w,   1.0);
            fill_rect(pixmap, &paint, x,         y + h - 1.0, w, 1.0);
            fill_rect(pixmap, &paint, x,         y,         1.0,   h);
            fill_rect(pixmap, &paint, x + w - 1.0, y,       1.0,   h);

            // Record shape for spatial queries
            let color = u32::from_be_bytes([b.0, b.1, b.2, 255]);
            shapes.push(RenderedShape {
                task_id: task.id,
                kind: ShapeKind::Rect,
                x, y, w, h,
                corner_radius: 0.0,
                color,
            });
        }

        shapes
    }
}

// ── Helpers ──────────────────────────────────────────────────────

#[inline]
fn fill_rect(pixmap: &mut Pixmap, paint: &Paint, x: f32, y: f32, w: f32, h: f32) {
    if let Some(rect) = Rect::from_xywh(x, y, w, h) {
        pixmap.fill_rect(rect, paint, Transform::identity(), None);
    }
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Command, Service, World};

    fn monday_this_week_mins() -> u32 {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mins = (secs / 60) as u32;
        let days = mins / 1440;
        let dow = (days + 3) % 7;
        (days - dow) * 1440
    }

    #[test]
    fn renders_empty_world_to_png() {
        let world = World::new();
        let mut renderer = Renderer::new(1316, 632);
        let window_start = monday_this_week_mins();
        let view = ViewState { window_start, window_end: window_start + 7 * 1440 };

        let output = renderer.render_world(&world, &view, ImageFormat::Png)
            .expect("render failed");

        assert!(!output.image_bytes.is_empty());
        assert_eq!(&output.image_bytes[1..4], b"PNG");
        assert_eq!(output.shapes.len(), 0);
    }

    #[test]
    fn chrome_cache_hit_on_second_render() {
        let world = World::new();
        let mut renderer = Renderer::new(1316, 632);
        let window_start = monday_this_week_mins();
        let view = ViewState { window_start, window_end: window_start + 7 * 1440 };

        assert!(!renderer.chrome_cache_hit(&view));

        renderer.render_world(&world, &view, ImageFormat::Png);
        assert!(renderer.chrome_cache_hit(&view));

        // Different view → cache miss
        let view2 = ViewState { window_start: window_start + 7 * 1440, window_end: window_start + 14 * 1440 };
        assert!(!renderer.chrome_cache_hit(&view2));
    }

    #[test]
    fn renders_tasks_with_shapes() {
        let mut world = World::new();
        let user_id = Uuid::new_v4();
        let service_id = Uuid::new_v4();
        let window_start = monday_this_week_mins();

        world.services.insert(service_id, Service { id: service_id, name: "Test".into() });

        for (col, hour, priority) in [
            (0u32, 9u32,  Priority::Medium),
            (1,    11,    Priority::High),
            (2,    14,    Priority::Urgent),
        ] {
            let _ = world.apply(
                Command::CreateTask {
                    title: format!("Task {col}"),
                    service_id,
                    priority,
                    assigned_to: None,
                    start: Some(window_start + col * 1440 + hour * 60),
                    duration: Some(60),
                },
                user_id,
            );
        }

        let mut renderer = Renderer::new(1316, 632);
        let view = ViewState { window_start, window_end: window_start + 7 * 1440 };

        let output = renderer.render_world(&world, &view, ImageFormat::Png).unwrap();

        assert_eq!(output.shapes.len(), 3);

        // Verify shape positions are within expected bounds
        for shape in &output.shapes {
            assert!(shape.x >= LEFT_GUTTER);
            assert!(shape.y >= TOP_HEADER);
            assert!(shape.w > 0.0);
            assert!(shape.h > 0.0);
            assert_eq!(shape.kind, ShapeKind::Rect);
        }
    }

    #[test]
    fn rendered_shape_positions_match_layout() {
        let mut world = World::new();
        let service_id = Uuid::new_v4();
        let window_start = monday_this_week_mins();

        world.services.insert(service_id, Service { id: service_id, name: "Test".into() });

        // Monday 9:00 AM, 1 hour
        let _ = world.apply(
            Command::CreateTask {
                title: "Check position".into(),
                service_id,
                priority: Priority::Medium,
                assigned_to: None,
                start: Some(window_start + 9 * 60),
                duration: Some(60),
            },
            Uuid::nil(),
        );

        let mut renderer = Renderer::new(1316, 632);
        let view = ViewState { window_start, window_end: window_start + 7 * 1440 };

        let output = renderer.render_world(&world, &view, ImageFormat::Png).unwrap();
        assert_eq!(output.shapes.len(), 1);

        let shape = &output.shapes[0];
        // Monday = column 0
        let expected_x = LEFT_GUTTER + TASK_PAD;
        let expected_y = TOP_HEADER + (9.0 - START_HOUR as f32) * HOUR_HEIGHT;
        let expected_w = DAY_WIDTH - TASK_PAD * 2.0;
        let expected_h = HOUR_HEIGHT; // 60 min = 1 hour

        assert!((shape.x - expected_x).abs() < 0.01);
        assert!((shape.y - expected_y).abs() < 0.01);
        assert!((shape.w - expected_w).abs() < 0.01);
        assert!((shape.h - expected_h).abs() < 0.01);
    }
}
