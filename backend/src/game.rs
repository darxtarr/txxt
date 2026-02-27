//! Game WebSocket handler.
//!
//! Binary protocol over WebSocket. Per-connection context tracks cursor,
//! viewport, and view mode. Message dispatch routes to spatial or mutation paths.
//!
//! Critical ordering: subscribe to broadcast BEFORE reading snapshot.
//! This ensures no events are missed between snapshot send and subscription.

use crate::renderer::{RenderOutput, ViewState};
use crate::spatial::{self, OverlayGenerator};
use crate::wire::{self, ClientMsg, ImageFormat, ViewMode};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};
use uuid::Uuid;

// ── Shared state ──────────────────────────────────────────────

pub type SharedState = Arc<AppState>;

pub struct AppState {
    pub world:        RwLock<crate::world::World>,
    pub save_file:    crate::persist::SaveFile,
    pub game_tx:      broadcast::Sender<Vec<u8>>,
    pub renderer:     Mutex<crate::renderer::Renderer>,
    pub view:         RwLock<ViewState>,
    pub shapes:       RwLock<Vec<crate::renderer::RenderedShape>>,
    pub overlay_gen:  Arc<dyn OverlayGenerator>,
    pub image_format: ImageFormat,
}

// ── Per-connection context ────────────────────────────────────

struct ConnContext {
    cursor: (f32, f32),
    viewport: (u16, u16),
    view_mode: ViewMode,
    user_id: Uuid,
    /// True if the last OverlayCommands sent to this client was non-empty.
    /// Used to gate sends: only transmit when overlay state changes.
    /// Empty→empty: send nothing (codec sleeps). Non-empty→empty: send clear.
    was_drawing: bool,
}

impl ConnContext {
    fn new(user_id: Uuid) -> Self {
        Self {
            cursor: (0.0, 0.0),
            viewport: (1316, 632),
            view_mode: ViewMode::Week,
            user_id,
            was_drawing: false,
        }
    }
}

// ── WS upgrade handler ───────────────────────────────────────

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

// ── Socket lifecycle ──────────────────────────────────────────

async fn handle_socket(socket: WebSocket, state: SharedState) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Step 1: Subscribe BEFORE snapshot (critical ordering).
    let mut broadcast_rx = state.game_tx.subscribe();

    // Step 2: Render + send initial background image.
    let initial_msg = {
        let world = state.world.read().await;
        let view = state.view.read().await;
        let mut renderer = state.renderer.lock().await;

        match renderer.render_world(&world, &view, state.image_format) {
            Some(output) => {
                // Update shared shapes
                let mut shapes = state.shapes.write().await;
                *shapes = output.shapes;

                wire::pack_background_image(
                    world.revision,
                    state.image_format,
                    renderer_width(&renderer),
                    renderer_height(&renderer),
                    &output.image_bytes,
                )
            }
            None => {
                eprintln!("[game] initial render failed");
                return;
            }
        }
    };

    if ws_tx.send(Message::Binary(initial_msg.into())).await.is_err() {
        return;
    }

    // Dev mode: use first user, or nil.
    let user_id = {
        let world = state.world.read().await;
        world.users.keys().next().copied().unwrap_or(Uuid::nil())
    };

    let ctx = Arc::new(tokio::sync::Mutex::new(ConnContext::new(user_id)));

    // Step 3: Spawn broadcast forwarder.
    let mut send_task = tokio::spawn(async move {
        while let Ok(bytes) = broadcast_rx.recv().await {
            if ws_tx.send(Message::Binary(bytes.into())).await.is_err() {
                break;
            }
        }
    });

    // Step 4: Process incoming messages.
    let mut recv_task = tokio::spawn({
        let state = state.clone();
        let ctx = ctx.clone();
        async move {
            while let Some(Ok(msg)) = ws_rx.next().await {
                match msg {
                    Message::Binary(data) => {
                        handle_message(&state, &ctx, &data).await;
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}

// ── Message dispatch ──────────────────────────────────────────

async fn handle_message(
    state: &SharedState,
    ctx: &tokio::sync::Mutex<ConnContext>,
    data: &[u8],
) {
    let msg = match wire::unpack_client_msg(data) {
        Ok(msg) => msg,
        Err(e) => {
            eprintln!("[game] bad message: {e}");
            return;
        }
    };

    match msg {
        ClientMsg::CursorMove { x, y } => {
            let (cursor, was_drawing) = {
                let mut c = ctx.lock().await;
                c.cursor = (x as f32, y as f32);
                (c.cursor, c.was_drawing)
            };

            // Spatial pass: read-lock only, no contention with mutations
            let shapes = state.shapes.read().await;
            let commands = state.overlay_gen.generate(cursor, 120.0, &shapes);

            // Only send when overlay state changes — codec sleeps otherwise.
            //   empty  + was_drawing  → send empty (clear the lines)
            //   filled + any          → send commands (draw/update)
            //   empty  + !was_drawing → send nothing (already clear)
            let now_drawing = !commands.is_empty();
            if now_drawing || was_drawing {
                let overlay_bytes = wire::pack_overlay_commands(&commands, 0x00FF00FF);
                let _ = state.game_tx.send(overlay_bytes);
                ctx.lock().await.was_drawing = now_drawing;
            }

            // Candidate list: only send when non-empty
            let world = state.world.read().await;
            let candidates = spatial::build_candidate_list(
                cursor, 120.0, &shapes,
                &|id| world.tasks.get(&id).map_or(String::new(), |t| t.title.clone()),
            );
            if !candidates.is_empty() {
                let candidate_bytes = wire::pack_candidate_list(&candidates);
                let _ = state.game_tx.send(candidate_bytes);
            }
        }

        ClientMsg::ViewportSize { width, height } => {
            let mut ctx = ctx.lock().await;
            ctx.viewport = (width, height);

            let mut renderer = state.renderer.lock().await;
            renderer.resize(width as u32, height as u32);
        }

        ClientMsg::SetView { mode } => {
            let mut ctx = ctx.lock().await;
            ctx.view_mode = mode;
        }

        ClientMsg::NavigateWindow { delta } => {
            let mut view = state.view.write().await;
            let step = 7 * 1440; // 1 week in minutes
            if delta > 0 {
                view.window_start += step;
                view.window_end += step;
            } else if delta < 0 {
                view.window_start = view.window_start.saturating_sub(step);
                view.window_end = view.window_end.saturating_sub(step);
            }
            drop(view);

            // Re-render after navigation
            re_render_and_broadcast(state).await;
        }

        ClientMsg::CreateAt { x, y } => {
            // Convert pixel position to start/duration
            let view = state.view.read().await;
            if let Some((start, duration)) = pixel_to_schedule(x, y, &view) {
                let svc_id = {
                    let world = state.world.read().await;
                    world.services.keys().next().copied().unwrap_or(Uuid::nil())
                };

                let user_id = ctx.lock().await.user_id;

                let event = {
                    let mut world = state.world.write().await;
                    world.apply(
                        crate::world::Command::CreateTask {
                            title: String::new(),
                            service_id: svc_id,
                            priority: crate::world::Priority::Medium,
                            assigned_to: None,
                            start: Some(start),
                            duration: Some(duration),
                        },
                        user_id,
                    )
                };

                if let Ok(event) = event {
                    let world = state.world.read().await;
                    if let Err(e) = state.save_file.flush(&world, &event) {
                        eprintln!("[game] save failed: {e}");
                    }
                    drop(world);
                    re_render_and_broadcast(state).await;
                }
            }
        }

        ClientMsg::DeleteTask { task_id: _short_id } => {
            // TODO: map short_id to UUID via shapes/world lookup
            // For phase 1, this is a stub
        }

        ClientMsg::CompleteTask { task_id: _short_id } => {
            // TODO: map short_id
        }

        ClientMsg::UnscheduleTask { task_id: _short_id } => {
            // TODO: map short_id
        }

        ClientMsg::ClickAt { .. }
        | ClientMsg::ModeChange { .. }
        | ClientMsg::VisibilityChange { .. }
        | ClientMsg::DropTask { .. }
        | ClientMsg::ResizeTask { .. }
        | ClientMsg::UpdateTitle { .. }
        | ClientMsg::SetPriority { .. } => {
            // Phase 1 stubs — wire in as features are built
        }
    }
}

// ── Re-render and broadcast ───────────────────────────────────

async fn re_render_and_broadcast(state: &SharedState) {
    let world = state.world.read().await;
    let view = state.view.read().await;
    let mut renderer = state.renderer.lock().await;

    if let Some(output) = renderer.render_world(&world, &view, state.image_format) {
        let msg = wire::pack_background_image(
            world.revision,
            state.image_format,
            renderer_width(&renderer),
            renderer_height(&renderer),
            &output.image_bytes,
        );

        let mut shapes = state.shapes.write().await;
        *shapes = output.shapes;
        drop(shapes);

        let _ = state.game_tx.send(msg);
    }
}

// ── Pixel → schedule conversion ───────────────────────────────

fn pixel_to_schedule(x: u16, y: u16, view: &ViewState) -> Option<(u32, u16)> {
    let xf = x as f32;
    let yf = y as f32;

    // Must be within grid area
    if xf < 56.0 || yf < 32.0 { return None; }

    let col = ((xf - 56.0) / 180.0) as u32;
    if col >= 7 { return None; }

    let hour_offset = (yf - 32.0) / 60.0;
    let minute_of_day = ((8.0 + hour_offset) * 60.0) as u32;

    // Snap to 15-minute grid
    let snapped = (minute_of_day / 15) * 15;

    let day_start = view.window_start / 1440;
    let start = (day_start + col) * 1440 + snapped;

    Some((start, 30)) // default 30 min duration
}

// ── Helpers ───────────────────────────────────────────────────

fn renderer_width(renderer: &crate::renderer::Renderer) -> u16 {
    renderer.width as u16
}

fn renderer_height(renderer: &crate::renderer::Renderer) -> u16 {
    renderer.height as u16
}
