//! Game WebSocket handler.
//!
//! Binary protocol over WebSocket using fixed-stride packed records.
//! See wire.rs for the byte layout — readable by JS DataView at known offsets.
//!
//! - Client sends: packed binary commands (wire::unpack_command)
//! - Server sends: packed binary snapshots + events (wire::pack_*)

use crate::auth::SharedState;
use crate::wire;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use uuid::Uuid;

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

    // Step 2: Read-lock World, pack binary snapshot, send to this client.
    let snapshot_bytes = {
        let world = state.world.read().unwrap();
        wire::pack_snapshot(&world)
    };

    if ws_tx.send(Message::Binary(snapshot_bytes.into())).await.is_err() {
        return; // client already gone
    }

    // Dev mode: use first user in World, or Uuid::nil if none.
    let user_id = {
        let world = state.world.read().unwrap();
        world.users.keys().next().copied().unwrap_or(Uuid::nil())
    };

    // Step 3: Spawn broadcast forwarder (sends events to this client).
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

/// Unpack a binary command, apply it to the World, flush to disk, broadcast the event.
/// All synchronous under the write lock — microseconds at this scale.
fn handle_command(state: &SharedState, data: &[u8], user_id: Uuid) {
    // Deserialize
    let cmd = match wire::unpack_command(data) {
        Ok(cmd) => cmd,
        Err(e) => {
            eprintln!("bad command from client: {e}");
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
                }
                event
            }
            Err(e) => {
                eprintln!("command rejected: {e:?}");
                return;
            }
        }
    };

    // Broadcast packed binary event to all connected clients.
    let bytes = wire::pack_event(&event);
    let _ = state.game_tx.send(bytes);
}
