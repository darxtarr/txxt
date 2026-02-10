//! Game WebSocket handler.
//!
//! Binary protocol over WebSocket:
//! - Client sends: postcard-encoded Command
//! - Server sends: postcard-encoded ServerMsg (Snapshot on connect, then Events)
//!
//! This is interim — Phase 4 will use fixed-stride packed structs for IRONCLAD's DataView.

use crate::auth::SharedState;
use crate::world::{Command, Event, Service, Task};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Wire messages ──────────────────────────────────────────────

/// Server → Client messages (postcard-encoded, sent as binary WS frames).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMsg {
    /// Sent once on connect: the full world state.
    Snapshot {
        revision: u64,
        tasks: Vec<Task>,
        services: Vec<Service>,
    },
    /// A mutation that just happened. Broadcast to all clients.
    Event(Event),
    /// Something went wrong with the client's command.
    Error { message: String },
}

// ── WS upgrade handler ────────────────────────────────────────

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

// ── Socket lifecycle ───────────────────────────────────────────

async fn handle_socket(socket: WebSocket, state: SharedState) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Step 1: Subscribe to broadcast BEFORE reading snapshot.
    // This ensures we don't miss events between snapshot and subscription.
    let mut broadcast_rx = state.game_tx.subscribe();

    // Step 2: Read-lock World, build snapshot, send to this client.
    let snapshot_bytes = {
        let world = state.world.read().unwrap();
        let msg = ServerMsg::Snapshot {
            revision: world.revision,
            tasks: world.tasks.values().cloned().collect(),
            services: world.services.values().cloned().collect(),
        };
        postcard::to_allocvec(&msg).unwrap()
    };

    if ws_tx.send(Message::Binary(snapshot_bytes.into())).await.is_err() {
        return; // client already gone
    }

    // Dev mode: use first user in World, or Uuid::nil if none.
    let user_id = {
        let world = state.world.read().unwrap();
        world.users.keys().next().copied().unwrap_or(Uuid::nil())
    };

    // Step 3: Spawn broadcast forwarder (sends events from other commands to this client).
    let mut send_task = tokio::spawn(async move {
        while let Ok(bytes) = broadcast_rx.recv().await {
            if ws_tx.send(Message::Binary(bytes.into())).await.is_err() {
                break;
            }
        }
    });

    // Step 4: Process incoming commands from this client.
    let mut recv_task = tokio::spawn({
        let state = state.clone();
        async move {
            while let Some(Ok(msg)) = ws_rx.next().await {
                match msg {
                    Message::Binary(data) => {
                        handle_command(&state, &data, user_id);
                    }
                    Message::Close(_) => break,
                    _ => {} // ignore text, ping, pong
                }
            }
        }
    });

    // Wait for either side to finish.
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}

// ── Command processing ─────────────────────────────────────────

/// Deserialize a command, apply it to the World, flush to disk, broadcast the event.
/// All synchronous under the write lock — microseconds at this scale.
fn handle_command(state: &SharedState, data: &[u8], user_id: Uuid) {
    // Deserialize
    let cmd: Command = match postcard::from_bytes(data) {
        Ok(cmd) => cmd,
        Err(e) => {
            eprintln!("bad command from client: {e}");
            // Could send ServerMsg::Error back, but we'd need the sender.
            // For now, just log and drop. The client sent garbage.
            return;
        }
    };

    // Apply (write-lock World)
    let event = {
        let mut world = state.world.write().unwrap();
        match world.apply(cmd, user_id) {
            Ok(event) => {
                // Flush to save file (sync, fast)
                if let Err(e) = state.save_file.flush(&world, &event) {
                    eprintln!("save file flush failed: {e}");
                    // World is mutated but disk is stale. At this scale,
                    // this is a crash-level problem. Log and continue.
                }
                event
            }
            Err(e) => {
                eprintln!("command rejected: {e:?}");
                // TODO: send Error back to the specific client
                return;
            }
        }
    };

    // Broadcast to all connected clients (including the sender — they'll
    // apply it like everyone else, keeping client logic uniform).
    let msg = ServerMsg::Event(event);
    if let Ok(bytes) = postcard::to_allocvec(&msg) {
        let _ = state.game_tx.send(bytes);
    }
}
