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
#[cfg(feature = "profile")]
use std::time::Instant;
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
    let mut socket = socket;
    #[cfg(feature = "profile")]
    tracing::info!("ws client connected");

    // Step 1: Subscribe to broadcast BEFORE reading snapshot.
    // This ensures we don't miss events between snapshot and subscription.
    let mut broadcast_rx = state.game_tx.subscribe();

    // Step 2: Read-lock World, pack binary snapshot, send to this client.
    #[cfg(feature = "profile")]
    let snapshot_start = Instant::now();
    let snapshot_bytes = {
        let world = state.world.read().unwrap();
        wire::pack_snapshot(&world)
    };
    #[cfg(feature = "profile")]
    tracing::debug!(elapsed_us = snapshot_start.elapsed().as_micros() as u64, bytes = snapshot_bytes.len(), "snapshot packed");

    #[cfg(feature = "profile")]
    let snapshot_send_start = Instant::now();
    if socket.send(Message::Binary(snapshot_bytes.into())).await.is_err() {
        return; // client already gone
    }
    #[cfg(feature = "profile")]
    tracing::debug!(elapsed_us = snapshot_send_start.elapsed().as_micros() as u64, "snapshot sent");

    // Dev mode: use first user in World, or Uuid::nil if none.
    let user_id = {
        let world = state.world.read().unwrap();
        world.users.keys().next().copied().unwrap_or(Uuid::nil())
    };

    // Step 3: Forward broadcasts and process client commands in one loop.
    loop {
        tokio::select! {
            recv = broadcast_rx.recv() => {
                match recv {
                    Ok(bytes) => {
                        if socket.send(Message::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        #[cfg(feature = "profile")]
                        tracing::warn!("ws client lagged behind broadcast channel");
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }

            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        handle_command(&state, &data, user_id);
                    }
                    Some(Ok(Message::Close(_))) => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => break,
                }
            }
        }
    }

    #[cfg(feature = "profile")]
    tracing::info!("ws client disconnected");
}

// ── Command processing ─────────────────────────────────────────

/// Unpack a binary command, apply it to the World, flush to disk, broadcast the event.
/// All synchronous under the write lock — microseconds at this scale.
fn handle_command(state: &SharedState, data: &[u8], user_id: Uuid) {
    #[cfg(feature = "profile")]
    let total_start = Instant::now();

    // Deserialize
    #[cfg(feature = "profile")]
    let unpack_start = Instant::now();
    let cmd = match wire::unpack_command(data) {
        Ok(cmd) => cmd,
        Err(e) => {
            eprintln!("bad command from client: {e}");
            #[cfg(feature = "profile")]
            tracing::warn!(error = %e, frame_len = data.len(), "bad command from client");
            return;
        }
    };

    #[cfg(feature = "profile")]
    tracing::debug!(elapsed_us = unpack_start.elapsed().as_micros() as u64, "command unpacked");

    // Apply (write-lock World)
    #[cfg(feature = "profile")]
    let lock_start = Instant::now();
    let event = {
        let mut world = state.world.write().unwrap();
        #[cfg(feature = "profile")]
        tracing::debug!(elapsed_us = lock_start.elapsed().as_micros() as u64, "world write lock acquired");

        #[cfg(feature = "profile")]
        let apply_start = Instant::now();
        match world.apply(cmd, user_id) {
            Ok(event) => {
                #[cfg(feature = "profile")]
                tracing::debug!(elapsed_us = apply_start.elapsed().as_micros() as u64, "world.apply completed");

                // Flush to save file (sync, fast)
                #[cfg(feature = "profile")]
                let flush_start = Instant::now();
                if let Err(e) = state.save_file.flush(&world, &event) {
                    eprintln!("save file flush failed: {e}");
                    #[cfg(feature = "profile")]
                    tracing::warn!(error = %e, "save file flush failed");
                }
                #[cfg(feature = "profile")]
                tracing::debug!(elapsed_us = flush_start.elapsed().as_micros() as u64, "save file flush completed");

                event
            }
            Err(e) => {
                eprintln!("command rejected: {e:?}");
                #[cfg(feature = "profile")]
                tracing::warn!(error = ?e, "command rejected");
                return;
            }
        }
    };

    // Broadcast packed binary event to all connected clients.
    #[cfg(feature = "profile")]
    let pack_start = Instant::now();
    let bytes = wire::pack_event(&event);
    #[cfg(feature = "profile")]
    tracing::debug!(elapsed_us = pack_start.elapsed().as_micros() as u64, "event packed");

    #[cfg(feature = "profile")]
    let tx_start = Instant::now();
    let _ = state.game_tx.send(bytes);
    #[cfg(feature = "profile")]
    tracing::debug!(elapsed_us = tx_start.elapsed().as_micros() as u64, total_us = total_start.elapsed().as_micros() as u64, "command pipeline complete");
}
