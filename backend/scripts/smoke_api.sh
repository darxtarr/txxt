#!/usr/bin/env bash
set -euo pipefail

# Smoke test for txxt backend API.
# Writes all artifacts under the repo to avoid sandbox issues.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP_DIR="$ROOT/tmp/txxt-smoke"
PID_FILE="$TMP_DIR/server.pid"

mkdir -p "$TMP_DIR"

cleanup() {
    if [ -f "$PID_FILE" ]; then
        pid="$(cat "$PID_FILE" || true)"
        if [ -n "${pid:-}" ] && kill -0 "$pid" 2>/dev/null; then
            kill "$pid" || true
            wait "$pid" 2>/dev/null || true
        fi
    fi
}
trap cleanup EXIT

cd "$ROOT/backend"
cargo build -q

cargo run -q &
echo $! > "$PID_FILE"

sleep 0.6

token="$(curl -sS -X POST http://127.0.0.1:3000/api/auth/login \
    -H 'Content-Type: application/json' \
    -d '{"username":"admin","password":"admin"}' \
    | jq -r .token)"

test -n "$token"

curl -sS http://127.0.0.1:3000/api/tasks \
    -H "Authorization: Bearer $token" \
    | jq 'length' > "$TMP_DIR/tasks_len.json"

new_id="$(curl -sS -X POST http://127.0.0.1:3000/api/tasks \
    -H "Authorization: Bearer $token" \
    -H 'Content-Type: application/json' \
    -d '{"title":"smoke test","description":"created by smoke_api.sh","status":"Pending","priority":"Low","category":null,"tags":[],"due_date":null,"assigned_to":null}' \
    | jq -r .id)"

test -n "$new_id"

curl -sS -X PUT "http://127.0.0.1:3000/api/tasks/$new_id" \
    -H "Authorization: Bearer $token" \
    -H 'Content-Type: application/json' \
    -d '{"status":"InProgress"}' \
    | jq -r .status > "$TMP_DIR/updated_status.txt"

curl -sS -X DELETE "http://127.0.0.1:3000/api/tasks/$new_id" \
    -H "Authorization: Bearer $token" \
    -o /dev/null -w '%{http_code}' > "$TMP_DIR/delete_code.txt"

printf "login token ok\n"
printf "tasks count: %s\n" "$(cat "$TMP_DIR/tasks_len.json")"
printf "update status: %s\n" "$(cat "$TMP_DIR/updated_status.txt")"
printf "delete http: %s\n" "$(cat "$TMP_DIR/delete_code.txt")"
