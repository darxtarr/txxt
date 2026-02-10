#!/usr/bin/env bash
set -euo pipefail

# Profiling smoke run for txxt backend.
#
# What it does:
# 1) Starts backend with profiling enabled
# 2) Opens one websocket client
# 3) Receives snapshot and extracts first service id
# 4) Sends N CreateTask commands (binary wire protocol)
# 5) Shuts down server and parses tracing logs
# 6) Writes a baseline summary JSON

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BACKEND_DIR="$ROOT/backend"
OUT_DIR="$ROOT/tmp/profile-smoke"
TS="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$OUT_DIR/$TS"
SERVER_LOG="$RUN_DIR/server.log"
CLIENT_JSON="$RUN_DIR/client-metrics.json"
SUMMARY_JSON="$RUN_DIR/baseline-summary.json"
PID_FILE="$RUN_DIR/server.pid"
SAVE_FILE="$RUN_DIR/tasks-profile.redb"

# Optional override for command count.
COUNT="${1:-80}"

mkdir -p "$RUN_DIR"

cleanup() {
  if [[ -f "$PID_FILE" ]]; then
    pid="$(cat "$PID_FILE" || true)"
    if [[ -n "${pid:-}" ]] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" || true
      wait "$pid" 2>/dev/null || true
    fi
  fi
}
trap cleanup EXIT

if [[ ! -x "$ROOT/.venv-profile/bin/python" ]]; then
  echo "Missing python venv at $ROOT/.venv-profile/bin/python"
  echo "Create it first (from repo root):"
  echo "  python3 -m venv .venv-profile && source .venv-profile/bin/activate && python -m pip install websockets"
  exit 1
fi

echo "[profile-smoke] run dir: $RUN_DIR"
echo "[profile-smoke] commands: $COUNT"
echo "[profile-smoke] save file: $SAVE_FILE"

if ss -ltn '( sport = :3000 )' | grep -q ':3000'; then
  echo "[profile-smoke] port 3000 already in use; stop the existing server first"
  exit 1
fi

cd "$BACKEND_DIR"
cargo build --features profile >/dev/null
TXXT_SAVE_FILE="$SAVE_FILE" RUST_LOG=txxt_server=debug "$BACKEND_DIR/target/debug/txxt-server" >"$SERVER_LOG" 2>&1 &
echo $! >"$PID_FILE"

# Wait for HTTP listener.
for _ in $(seq 1 60); do
  if curl -fsS -o /dev/null "http://127.0.0.1:3000" 2>/dev/null; then
    break
  fi
  sleep 0.2
done

if ! curl -fsS -o /dev/null "http://127.0.0.1:3000" 2>/dev/null; then
  echo "[profile-smoke] server failed to start; tailing log:"
  tail -n 40 "$SERVER_LOG" || true
  exit 1
fi

"$ROOT/.venv-profile/bin/python" - <<'PY' "$COUNT" "$CLIENT_JSON"
import asyncio
import json
import struct
import sys
import time

import websockets

count = int(sys.argv[1])
client_json = sys.argv[2]

SNAPSHOT = 0x01
TASK_CREATED = 0x02
CMD_CREATE_TASK = 0x10
TASK_STRIDE = 192
SERVICE_STRIDE = 80


def pctl(values, pct):
    if not values:
        return None
    vals = sorted(values)
    idx = int(round((pct / 100.0) * (len(vals) - 1)))
    return vals[idx]


def build_create(service_id: bytes, idx: int) -> bytes:
    title = f"profile smoke task {idx}".encode("utf-8")
    # [0] type, [1] priority, [2..18] service_id, [18..34] assigned_to nil, [34..] title
    return bytes([CMD_CREATE_TASK, 1]) + service_id + (b"\x00" * 16) + title


async def run() -> None:
    uri = "ws://127.0.0.1:3000/api/game"
    sends_us = []
    recv_us = []

    async with websockets.connect(uri, max_size=4_000_000) as ws:
        snapshot = await ws.recv()
        if not isinstance(snapshot, (bytes, bytearray)):
            raise RuntimeError("expected binary snapshot frame")

        if snapshot[0] != SNAPSHOT:
            raise RuntimeError(f"expected snapshot(0x01), got 0x{snapshot[0]:02x}")

        task_count = struct.unpack_from("<I", snapshot, 9)[0]
        service_count = struct.unpack_from("<I", snapshot, 13)[0]
        if service_count == 0:
            raise RuntimeError("snapshot has zero services; cannot issue CreateTask")

        service_offset = 17 + (task_count * TASK_STRIDE)
        service_id = bytes(snapshot[service_offset:service_offset + 16])

        created = 0
        for i in range(count):
            frame = build_create(service_id, i)
            t0 = time.perf_counter_ns()
            await ws.send(frame)
            t1 = time.perf_counter_ns()
            sends_us.append((t1 - t0) // 1000)

        while created < count:
            msg = await ws.recv()
            t2 = time.perf_counter_ns()
            if isinstance(msg, (bytes, bytearray)) and len(msg) > 0 and msg[0] == TASK_CREATED:
                recv_us.append(0)
                # recv timing here is mostly loop/await timing; server metrics from tracing are canonical.
                created += 1

        out = {
            "count": count,
            "snapshot": {
                "task_count": task_count,
                "service_count": service_count,
            },
            "client_send_us": {
                "p50": pctl(sends_us, 50),
                "p95": pctl(sends_us, 95),
                "max": max(sends_us) if sends_us else None,
            },
            "created_events_received": created,
            "ts_ns": t2,
        }

        with open(client_json, "w", encoding="utf-8") as f:
            json.dump(out, f, indent=2)


asyncio.run(run())
PY

# Graceful stop and ensure all logs are flushed.
if [[ -f "$PID_FILE" ]]; then
  pid="$(cat "$PID_FILE")"
  if kill -0 "$pid" 2>/dev/null; then
    kill "$pid" || true
    wait "$pid" 2>/dev/null || true
  fi
fi

"$ROOT/.venv-profile/bin/python" - <<'PY' "$SERVER_LOG" "$CLIENT_JSON" "$SUMMARY_JSON"
import json
import re
import sys

server_log, client_json, summary_json = sys.argv[1:4]

ansi = re.compile(r"\x1b\[[0-9;]*m")

rx = {
    "pipeline_total_us": re.compile(r"(?:command pipeline complete.*total_us=(\d+)|total_us=(\d+).+command pipeline complete)"),
    "lock_wait_us": re.compile(r"(?:world write lock acquired.*elapsed_us=(\d+)|elapsed_us=(\d+).+world write lock acquired)"),
    "apply_us": re.compile(r"(?:world\.apply completed.*elapsed_us=(\d+)|elapsed_us=(\d+).+world\.apply completed)"),
    "flush_us": re.compile(r"(?:save file flush completed.*elapsed_us=(\d+)|elapsed_us=(\d+).+save file flush completed)"),
    "pack_us": re.compile(r"(?:event packed.*elapsed_us=(\d+)|elapsed_us=(\d+).+event packed)"),
    "snapshot_pack_us": re.compile(r"(?:snapshot packed.*elapsed_us=(\d+)|elapsed_us=(\d+).+snapshot packed)"),
}

vals = {k: [] for k in rx}

with open(server_log, "r", encoding="utf-8", errors="replace") as f:
    for line in f:
        line = ansi.sub("", line)
        for k, pat in rx.items():
            m = pat.search(line)
            if m:
                g1, g2 = m.group(1), m.group(2)
                vals[k].append(int(g1 or g2))


def pctl(xs, pct):
    if not xs:
        return None
    xs = sorted(xs)
    idx = int(round((pct / 100.0) * (len(xs) - 1)))
    return xs[idx]


def stats(xs):
    return {
        "n": len(xs),
        "p50": pctl(xs, 50),
        "p95": pctl(xs, 95),
        "max": max(xs) if xs else None,
    }


with open(client_json, "r", encoding="utf-8") as f:
    client = json.load(f)

summary = {
    "profile_smoke": {
        "commands_sent": client.get("count"),
        "created_events_received": client.get("created_events_received"),
        "snapshot": client.get("snapshot"),
    },
    "server_metrics_us": {
        key: stats(values) for key, values in vals.items()
    },
    "client_metrics_us": {
        "send": client.get("client_send_us", {}),
    },
}

with open(summary_json, "w", encoding="utf-8") as f:
    json.dump(summary, f, indent=2)

print(json.dumps(summary, indent=2))
PY

echo
echo "[profile-smoke] server log:      $SERVER_LOG"
echo "[profile-smoke] client metrics:  $CLIENT_JSON"
echo "[profile-smoke] baseline json:   $SUMMARY_JSON"
