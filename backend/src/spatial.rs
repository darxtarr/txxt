//! Spatial queries: SDF evaluation, clip geometry, overlay generation.
//!
//! This is a direct port of the JS testbed math (frontend/ironclad.js TestHarness).
//! The OverlayGenerator trait is THE modularity seam — swap brute force for
//! grid broadphase, temporal coherence, etc. without touching anything else.

use crate::renderer::{RenderedShape, ShapeKind};
use crate::wire::{Candidate, OverlayCommand};
use uuid::Uuid;

// ── The trait ─────────────────────────────────────────────────

/// Generates overlay draw commands for the flashlight effect.
/// Phase 1: SdfOverlay (O(n) brute force, direct port of JS testbed).
/// Future: grid broadphase, temporal coherence, Fitts ranking.
pub trait OverlayGenerator: Send + Sync {
    fn generate(
        &self,
        cursor: (f32, f32),
        radius: f32,
        shapes: &[RenderedShape],
    ) -> Vec<OverlayCommand>;
}

// ── SDF functions ─────────────────────────────────────────────

/// Signed distance from point to rectangle boundary.
/// Negative = inside, positive = outside.
fn sdf_rect(px: f32, py: f32, rx: f32, ry: f32, rw: f32, rh: f32) -> f32 {
    let cx = rx + rw / 2.0;
    let cy = ry + rh / 2.0;
    let dx = (px - cx).abs() - rw / 2.0;
    let dy = (py - cy).abs() - rh / 2.0;
    let outside = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt();
    let inside = dx.max(dy).min(0.0);
    outside + inside
}

/// Signed distance from point to rounded rectangle boundary.
fn sdf_rounded_rect(px: f32, py: f32, rx: f32, ry: f32, rw: f32, rh: f32, cr: f32) -> f32 {
    let cx = rx + rw / 2.0;
    let cy = ry + rh / 2.0;
    let hw = rw / 2.0 - cr;
    let hh = rh / 2.0 - cr;
    let dx = ((px - cx).abs() - hw).max(0.0);
    let dy = ((py - cy).abs() - hh).max(0.0);
    (dx * dx + dy * dy).sqrt() - cr
}

/// Evaluate SDF for a rendered shape.
fn sdf_shape(px: f32, py: f32, shape: &RenderedShape) -> f32 {
    match shape.kind {
        ShapeKind::Rect => sdf_rect(px, py, shape.x, shape.y, shape.w, shape.h),
        ShapeKind::RoundedRect => sdf_rounded_rect(
            px, py, shape.x, shape.y, shape.w, shape.h, shape.corner_radius,
        ),
    }
}

// ── Edge decomposition ────────────────────────────────────────

#[derive(Debug, Clone)]
enum Edge {
    Segment { x1: f32, y1: f32, x2: f32, y2: f32 },
    Arc { cx: f32, cy: f32, r: f32, start_angle: f32, sweep_angle: f32 },
}

/// Decompose a shape into its edge primitives.
fn shape_edges(shape: &RenderedShape) -> Vec<Edge> {
    let (x, y, w, h) = (shape.x, shape.y, shape.w, shape.h);

    match shape.kind {
        ShapeKind::Rect => vec![
            Edge::Segment { x1: x, y1: y, x2: x + w, y2: y },
            Edge::Segment { x1: x + w, y1: y, x2: x + w, y2: y + h },
            Edge::Segment { x1: x + w, y1: y + h, x2: x, y2: y + h },
            Edge::Segment { x1: x, y1: y + h, x2: x, y2: y },
        ],
        ShapeKind::RoundedRect => {
            let cr = shape.corner_radius.min(w / 2.0).min(h / 2.0);
            use std::f32::consts::PI;
            vec![
                // Straight edges between corners
                Edge::Segment { x1: x + cr, y1: y, x2: x + w - cr, y2: y },
                Edge::Segment { x1: x + w, y1: y + cr, x2: x + w, y2: y + h - cr },
                Edge::Segment { x1: x + w - cr, y1: y + h, x2: x + cr, y2: y + h },
                Edge::Segment { x1: x, y1: y + h - cr, x2: x, y2: y + cr },
                // Corner arcs
                Edge::Arc { cx: x + w - cr, cy: y + cr, r: cr, start_angle: -PI / 2.0, sweep_angle: PI / 2.0 },
                Edge::Arc { cx: x + w - cr, cy: y + h - cr, r: cr, start_angle: 0.0, sweep_angle: PI / 2.0 },
                Edge::Arc { cx: x + cr, cy: y + h - cr, r: cr, start_angle: PI / 2.0, sweep_angle: PI / 2.0 },
                Edge::Arc { cx: x + cr, cy: y + cr, r: cr, start_angle: PI, sweep_angle: PI / 2.0 },
            ]
        }
    }
}

// ── Clip geometry ─────────────────────────────────────────────

/// Clip a line segment to the interior of a circle.
/// Returns None if no intersection, or the clipped sub-segment.
fn clip_segment_to_circle(
    sx1: f32, sy1: f32, sx2: f32, sy2: f32,
    cx: f32, cy: f32, cr: f32,
) -> Option<(f32, f32, f32, f32)> {
    let dx = sx2 - sx1;
    let dy = sy2 - sy1;
    let fx = sx1 - cx;
    let fy = sy1 - cy;

    let a = dx * dx + dy * dy;
    if a < 1e-10 {
        let d2 = fx * fx + fy * fy;
        return if d2 <= cr * cr { Some((sx1, sy1, sx2, sy2)) } else { None };
    }

    let b = 2.0 * (fx * dx + fy * dy);
    let c = fx * fx + fy * fy - cr * cr;
    let disc = b * b - 4.0 * a * c;

    if disc < 0.0 {
        let d2 = fx * fx + fy * fy;
        return if d2 <= cr * cr { Some((sx1, sy1, sx2, sy2)) } else { None };
    }

    let sqrt_disc = disc.sqrt();
    let t0 = ((-b - sqrt_disc) / (2.0 * a)).max(0.0);
    let t1 = ((-b + sqrt_disc) / (2.0 * a)).min(1.0);

    if t0 >= t1 { return None; }

    Some((
        sx1 + t0 * dx,
        sy1 + t0 * dy,
        sx1 + t1 * dx,
        sy1 + t1 * dy,
    ))
}

/// Clip an arc to the interior of a circle.
/// Returns None or the clipped arc (center, radius, start_angle, sweep_angle).
fn clip_arc_to_circle(
    acx: f32, acy: f32, ar: f32,
    start_angle: f32, sweep_angle: f32,
    cx: f32, cy: f32, cr: f32,
) -> Option<(f32, f32, f32, f32, f32)> {
    let ddx = acx - cx;
    let ddy = acy - cy;
    let dist = (ddx * ddx + ddy * ddy).sqrt();

    // Arc circle entirely inside flashlight
    if dist + ar <= cr {
        return Some((acx, acy, ar, start_angle, sweep_angle));
    }

    // No intersection
    if dist >= ar + cr || dist + cr <= ar {
        return None;
    }

    // Two-circle intersection
    let cos_alpha = (dist * dist + ar * ar - cr * cr) / (2.0 * dist * ar);
    let alpha = cos_alpha.clamp(-1.0, 1.0).acos();
    let center_angle = (cy - acy).atan2(cx - acx);

    let inside_start = center_angle - alpha;
    let inside_end = center_angle + alpha;

    let arc_start = start_angle;
    let arc_end = start_angle + sweep_angle;

    let result = intersect_angle_ranges(arc_start, arc_end, inside_start, inside_end)?;

    Some((acx, acy, ar, result.0, result.1 - result.0))
}

/// Intersect two angle ranges, handling wrap-around.
fn intersect_angle_ranges(
    a_start: f32, a_end: f32,
    b_start: f32, b_end: f32,
) -> Option<(f32, f32)> {
    let two_pi = std::f32::consts::TAU;
    let mut best: Option<(f32, f32)> = None;

    for offset in [-two_pi, 0.0, two_pi] {
        let bs = b_start + offset;
        let be = b_end + offset;
        let s = a_start.max(bs);
        let e = a_end.min(be);
        if e > s + 1e-6 {
            if best.map_or(true, |b| (e - s) > (b.1 - b.0)) {
                best = Some((s, e));
            }
        }
    }
    best
}

// ── SdfOverlay implementation ─────────────────────────────────

/// Brute-force O(n) overlay generator. Direct port of JS testbed.
pub struct SdfOverlay;

impl OverlayGenerator for SdfOverlay {
    fn generate(
        &self,
        cursor: (f32, f32),
        radius: f32,
        shapes: &[RenderedShape],
    ) -> Vec<OverlayCommand> {
        let (mx, my) = cursor;
        let mut commands = Vec::new();

        for shape in shapes {
            let d = sdf_shape(mx, my, shape);

            // Shape boundary within flashlight radius (with margin for edge thickness)
            if d.abs() >= radius + 10.0 {
                continue;
            }

            let edges = shape_edges(shape);

            for edge in &edges {
                match edge {
                    Edge::Segment { x1, y1, x2, y2 } => {
                        if let Some((cx1, cy1, cx2, cy2)) =
                            clip_segment_to_circle(*x1, *y1, *x2, *y2, mx, my, radius)
                        {
                            commands.push(OverlayCommand::Segment {
                                x1: cx1, y1: cy1,
                                x2: cx2, y2: cy2,
                            });
                        }
                    }
                    Edge::Arc { cx, cy, r, start_angle, sweep_angle } => {
                        if let Some((acx, acy, ar, sa, sw)) =
                            clip_arc_to_circle(*cx, *cy, *r, *start_angle, *sweep_angle, mx, my, radius)
                        {
                            commands.push(OverlayCommand::Arc {
                                cx: acx, cy: acy, r: ar,
                                start_angle: sa, sweep_angle: sw,
                            });
                        }
                    }
                }
            }
        }

        commands
    }
}

// ── Candidate list builder ────────────────────────────────────

/// Build the candidate list for click semantics.
/// Not behind a trait — this is always the same logic.
pub fn build_candidate_list(
    cursor: (f32, f32),
    radius: f32,
    shapes: &[RenderedShape],
    title_lookup: &dyn Fn(Uuid) -> String,
) -> Vec<Candidate> {
    let (mx, my) = cursor;
    let mut candidates = Vec::new();

    for shape in shapes {
        let d = sdf_shape(mx, my, shape);
        if d.abs() >= radius + 10.0 {
            continue;
        }

        candidates.push(Candidate {
            task_id: shape.task_id,
            x: shape.x as u16,
            y: shape.y as u16,
            w: shape.w as u16,
            h: shape.h as u16,
            color: shape.color,
            sdf_dist: (d * 10.0) as i16, // 0.1px precision
            title: title_lookup(shape.task_id),
        });
    }

    // Sort by absolute distance (closest first)
    candidates.sort_by(|a, b| a.sdf_dist.abs().cmp(&b.sdf_dist.abs()));
    candidates
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rect(x: f32, y: f32, w: f32, h: f32) -> RenderedShape {
        RenderedShape {
            task_id: Uuid::new_v4(),
            kind: ShapeKind::Rect,
            x, y, w, h,
            corner_radius: 0.0,
            color: 0x3a7bd5ff,
        }
    }

    fn make_rrect(x: f32, y: f32, w: f32, h: f32, cr: f32) -> RenderedShape {
        RenderedShape {
            task_id: Uuid::new_v4(),
            kind: ShapeKind::RoundedRect,
            x, y, w, h,
            corner_radius: cr,
            color: 0x2d9b7aff,
        }
    }

    // ── SDF tests ────────────────────────────────────────────

    #[test]
    fn sdf_rect_center_is_negative() {
        // Point at center of 100x100 rect at (0,0)
        let d = sdf_rect(50.0, 50.0, 0.0, 0.0, 100.0, 100.0);
        assert!(d < 0.0, "center of rect should be negative SDF: {d}");
        assert!((d - (-50.0)).abs() < 0.01);
    }

    #[test]
    fn sdf_rect_edge_is_zero() {
        // Point on the edge
        let d = sdf_rect(0.0, 50.0, 0.0, 0.0, 100.0, 100.0);
        assert!(d.abs() < 0.01, "edge should be ~0: {d}");
    }

    #[test]
    fn sdf_rect_outside_is_positive() {
        let d = sdf_rect(-10.0, 50.0, 0.0, 0.0, 100.0, 100.0);
        assert!(d > 0.0, "outside should be positive: {d}");
        assert!((d - 10.0).abs() < 0.01);
    }

    #[test]
    fn sdf_rounded_rect_corner() {
        // Point at corner of a rounded rect — should be at distance from the arc
        let cr = 10.0;
        let d = sdf_rounded_rect(0.0, 0.0, 0.0, 0.0, 100.0, 100.0, cr);
        // At exact corner, distance should be positive (outside the rounded corner)
        assert!(d > 0.0, "corner of rrect should be outside: {d}");
    }

    // ── Clip geometry tests ──────────────────────────────────

    #[test]
    fn clip_segment_fully_inside() {
        // Short segment well within circle
        let result = clip_segment_to_circle(45.0, 50.0, 55.0, 50.0, 50.0, 50.0, 100.0);
        let (x1, y1, x2, y2) = result.unwrap();
        assert!((x1 - 45.0).abs() < 0.01);
        assert!((x2 - 55.0).abs() < 0.01);
    }

    #[test]
    fn clip_segment_partially_inside() {
        // Long horizontal segment through circle center
        let result = clip_segment_to_circle(-200.0, 0.0, 200.0, 0.0, 0.0, 0.0, 50.0);
        let (x1, _, x2, _) = result.unwrap();
        assert!((x1 - (-50.0)).abs() < 0.5);
        assert!((x2 - 50.0).abs() < 0.5);
    }

    #[test]
    fn clip_segment_fully_outside() {
        let result = clip_segment_to_circle(200.0, 200.0, 300.0, 200.0, 0.0, 0.0, 50.0);
        assert!(result.is_none());
    }

    #[test]
    fn clip_arc_fully_inside() {
        // Small arc well within flashlight
        let result = clip_arc_to_circle(
            50.0, 50.0, 5.0, // tiny arc
            0.0, std::f32::consts::PI,
            50.0, 50.0, 100.0, // big flashlight
        );
        let (_, _, _, sa, sw) = result.unwrap();
        assert!((sa - 0.0).abs() < 0.01);
        assert!((sw - std::f32::consts::PI).abs() < 0.01);
    }

    #[test]
    fn clip_arc_fully_outside() {
        let result = clip_arc_to_circle(
            500.0, 500.0, 10.0,
            0.0, std::f32::consts::PI,
            0.0, 0.0, 50.0,
        );
        assert!(result.is_none());
    }

    // ── Overlay generator tests ──────────────────────────────

    #[test]
    fn sdf_overlay_generates_segments_for_rect() {
        let overlay = SdfOverlay;
        let shapes = vec![make_rect(100.0, 100.0, 160.0, 60.0)];

        // Cursor near the center of the rect
        let commands = overlay.generate((180.0, 130.0), 120.0, &shapes);

        // Should produce segment commands (rect has 4 edges, some clipped)
        assert!(!commands.is_empty(), "should produce overlay commands");

        for cmd in &commands {
            match cmd {
                OverlayCommand::Segment { .. } => {}
                _ => panic!("rect should only produce segments"),
            }
        }
    }

    #[test]
    fn sdf_overlay_empty_when_cursor_far_away() {
        let overlay = SdfOverlay;
        let shapes = vec![make_rect(100.0, 100.0, 160.0, 60.0)];

        let commands = overlay.generate((1000.0, 1000.0), 120.0, &shapes);
        assert!(commands.is_empty());
    }

    #[test]
    fn sdf_overlay_handles_rounded_rect() {
        let overlay = SdfOverlay;
        let shapes = vec![make_rrect(100.0, 100.0, 160.0, 60.0, 8.0)];

        let commands = overlay.generate((180.0, 130.0), 120.0, &shapes);
        assert!(!commands.is_empty());

        let has_arc = commands.iter().any(|c| matches!(c, OverlayCommand::Arc { .. }));
        let has_segment = commands.iter().any(|c| matches!(c, OverlayCommand::Segment { .. }));
        assert!(has_arc, "rounded rect should produce arcs");
        assert!(has_segment, "rounded rect should produce segments");
    }

    #[test]
    fn candidate_list_sorted_by_distance() {
        let shapes = vec![
            make_rect(100.0, 100.0, 160.0, 60.0), // closer to cursor
            make_rect(300.0, 300.0, 160.0, 60.0), // farther
        ];

        let candidates = build_candidate_list(
            (180.0, 130.0),
            500.0, // big radius to catch both
            &shapes,
            &|_| "test".to_string(),
        );

        assert!(candidates.len() >= 1);
        if candidates.len() >= 2 {
            assert!(candidates[0].sdf_dist.abs() <= candidates[1].sdf_dist.abs());
        }
    }
}
