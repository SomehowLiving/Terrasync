#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

LOG_DIR="${LOG_DIR:-$ROOT_DIR/logs/demo}"
mkdir -p "$LOG_DIR"
rm -f "$LOG_DIR"/*.log "$LOG_DIR"/*.pid

AGENTS=(a1 a2 a3 a4 a5)
REGION="${CONFLICT_REGION:-0,0}"
BROKERS="${FOXMQ_BROKERS:-127.0.0.1:1883}"
MQTT_USERNAME="${FOXMQ_USERNAME:-}"
MQTT_PASSWORD="${FOXMQ_PASSWORD:-}"

REGION_HASH="$(printf '%s' "${REGION/,/:}" | sha256sum | awk '{print $1}')"
REGION_SHORT="${REGION_HASH:0:8}"

cleanup() {
  for f in "$LOG_DIR"/*.pid; do
    [[ -e "$f" ]] || continue
    pid=$(cat "$f" || true)
    if [[ -n "${pid:-}" ]] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
    fi
  done
}
trap cleanup EXIT

check_broker_reachable() {
  local endpoint="$1"
  local host port
  host="${endpoint%:*}"
  port="${endpoint##*:}"
  if [[ -z "$host" || -z "$port" || "$host" == "$port" ]]; then
    echo "invalid broker endpoint '$endpoint' (expected host:port)"
    return 1
  fi
  timeout 1 bash -c ">/dev/tcp/$host/$port" 2>/dev/null
}

cargo build >/dev/null

echo "using FoxMQ brokers: $BROKERS"
if [[ -n "$MQTT_USERNAME" ]]; then
  echo "using FoxMQ username: $MQTT_USERNAME"
else
  echo "FOXMQ_USERNAME not set; attempting anonymous MQTT connection"
fi

reachable=0
IFS=',' read -ra broker_arr <<< "$BROKERS"
for endpoint in "${broker_arr[@]}"; do
  if check_broker_reachable "$endpoint"; then
    echo "broker reachable: $endpoint"
    reachable=1
  else
    echo "broker unreachable: $endpoint"
  fi
done
if [[ "$reachable" -eq 0 ]]; then
  echo "no reachable FoxMQ broker endpoints; start FoxMQ and/or set FOXMQ_BROKERS correctly"
  exit 1
fi

for id in "${AGENTS[@]}"; do
  NO_COLOR=1 RUST_LOG=info RUST_LOG_STYLE=never ./target/debug/vertex-hack \
    --agent-id "$id" \
    --brokers "$BROKERS" \
    --mqtt-username "$MQTT_USERNAME" \
    --mqtt-password "$MQTT_PASSWORD" \
    --grid-size 10 \
    --heartbeat-ms 400 \
    --heartbeat-timeout-ms 1600 \
    --claim-round-ms 1500 \
    --tick-ms 300 \
    --force-conflict-region "$REGION" \
    >"$LOG_DIR/$id.log" 2>&1 &

  echo $! > "$LOG_DIR/$id.pid"
  echo "started $id"
done

sleep 5

if ! grep -h "connected to broker" "$LOG_DIR"/*.log >/dev/null; then
  echo "agents started but none connected to broker; check FOXMQ_USERNAME/FOXMQ_PASSWORD and FoxMQ auth config"
  exit 1
fi

echo ""
echo "--- contention evidence ---"
grep -h "claims received set.*region=$REGION_SHORT" "$LOG_DIR"/*.log | head -n 10 || true
grep -h "deterministic winner computed.*region=$REGION_SHORT" "$LOG_DIR"/*.log | head -n 10 || true
grep -h "claim lost, rerouting immediately.*region=$REGION_SHORT" "$LOG_DIR"/*.log | head -n 10 || true

winner="$(grep -h "deterministic winner computed.*region=$REGION_SHORT" "$LOG_DIR"/*.log | head -n 1 | sed -E 's/.* winner=([^ ]+).*/\1/' || true)"
if [[ -z "$winner" ]]; then
  echo "no winner found; ensure FoxMQ cluster is reachable and inspect $LOG_DIR"
  exit 1
fi

echo ""
echo "selected winner to kill: $winner"
kill "$(cat "$LOG_DIR/$winner.pid")"
rm -f "$LOG_DIR/$winner.pid"

sleep 5

echo ""
echo "--- failover evidence ---"
grep -hE "heartbeat timeout|owner failed, region reopened|claim won, proceeding.*region=$REGION_SHORT" "$LOG_DIR"/*.log | tail -n 40 || true

echo ""
echo "logs written to: $LOG_DIR"
echo "agents still running:"
for f in "$LOG_DIR"/*.pid; do
  [[ -e "$f" ]] || continue
  id="$(basename "$f" .pid)"
  pid="$(cat "$f")"
  if kill -0 "$pid" 2>/dev/null; then
    echo "  $id ($pid)"
  fi
done
