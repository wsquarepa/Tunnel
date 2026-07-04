#!/usr/bin/env bash
# KEYSTONE end-to-end test: HTTP + SSE + WebSocket + two-client pool through the
# full stack (dummy origin <- tunnel-client <- wrangler dev --local worker <- curl/node).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
PORT=8787
SLUG=demo
TARGET=demo
BASE="http://127.0.0.1:$PORT"
ORIGIN_PORT=9099
CFG=/tmp/tunnel_e2e.toml
WLOG=/tmp/e2e_wrangler.log
OLOG=/tmp/e2e_origin.log
CLOG=/tmp/e2e_client.log

log() { echo "[e2e] $*"; }

cleanup() {
    kill "${CLIENT_PID:-}" "${CLIENT2_PID:-}" "${WRANGLER_PID:-}" \
         "${ORIGIN_PID:-}" "${ORIGIN2_PID:-}" 2>/dev/null || true
}
trap cleanup EXIT

# --- Stage 1: dummy origin ------------------------------------------------
log "STAGE origin: starting dummy origin on 127.0.0.1:$ORIGIN_PORT"
DUMMY_ORIGIN_ID=origin-a cargo run -q -p tunnel-client --example dummy_origin >"$OLOG" 2>&1 &
ORIGIN_PID=$!
for _ in $(seq 1 60); do
    if curl -s -o /dev/null -d probe "http://127.0.0.1:$ORIGIN_PORT/echo"; then
        log "STAGE origin: ready"
        break
    fi
    sleep 1
done
curl -s -o /dev/null -d probe "http://127.0.0.1:$ORIGIN_PORT/echo" \
    || { log "FAIL origin never came up"; cat "$OLOG"; exit 1; }

# --- Stage 2: worker (wrangler dev --local) -------------------------------
log "STAGE worker: writing .dev.vars and applying D1 migration"
printf 'ADMIN_SECRET = "test"\n' >"$ROOT/crates/tunnel-worker/.dev.vars"
# Start from a clean local persistence so provisioning is deterministic across
# reruns (otherwise the UNIQUE(kind,matcher) route from a prior run collides).
rm -rf "$ROOT/crates/tunnel-worker/.wrangler/state/v3/d1" \
       "$ROOT/crates/tunnel-worker/.wrangler/state/v3/do"
( cd "$ROOT/crates/tunnel-worker" && npx wrangler d1 migrations apply tunnel --local ) \
    >/tmp/e2e_migrate.log 2>&1 || { log "FAIL D1 migration"; cat /tmp/e2e_migrate.log; exit 1; }

log "STAGE worker: building admin panel"
( cd "$ROOT/crates/tunnel-worker/panel" && npm ci && npm run build )

log "STAGE worker: starting wrangler dev --local on :$PORT"
( cd "$ROOT/crates/tunnel-worker" && npx wrangler dev --local --port "$PORT" ) >"$WLOG" 2>&1 &
WRANGLER_PID=$!
worker_ready=""
for _ in $(seq 1 40); do
    # Any HTTP response (even 404) means the worker is serving.
    if curl -s -o /dev/null "$BASE/"; then
        worker_ready=1
        break
    fi
    sleep 2
done
[ -n "$worker_ready" ] || { log "FAIL worker never became ready"; cat "$WLOG"; exit 1; }
log "STAGE worker: ready"

# --- Stage 3: provision client + route via admin API ----------------------
log "STAGE provision: admin login"
# The session cookie is set with `Secure`, which curl refuses to resend over
# plain http. Extract the raw value and pass it as a Cookie header ourselves.
COOKIE=$(curl -s -D - -o /dev/null "$BASE/admin/login" \
    -H 'Content-Type: application/json' -d '{"secret":"test"}' \
    | grep -i '^set-cookie:' | sed -E 's/.*tunnel_session=([^;]*).*/\1/' | tr -d '\r')
[ -n "$COOKIE" ] || { log "FAIL admin login (no cookie)"; cat "$WLOG"; exit 1; }

auth=(-H "Cookie: tunnel_session=$COOKIE" -H 'X-Tunnel-CSRF: 1' -H 'Content-Type: application/json')

log "STAGE provision: create client"
CREATED=$(curl -s "$BASE/admin/clients" "${auth[@]}" -d '{"name":"e2e"}')
TOKEN=$(echo "$CREATED" | jq -r '.token')
CLIENT_ID=$(echo "$CREATED" | jq -r '.id')
[ -n "$TOKEN" ] && [ "$TOKEN" != "null" ] || { log "FAIL create client: $CREATED"; exit 1; }

log "STAGE provision: create route $SLUG -> $TARGET (client $CLIENT_ID)"
ROUTE=$(curl -s "$BASE/admin/routes" "${auth[@]}" \
    -d "{\"client_id\":\"$CLIENT_ID\",\"kind\":\"path\",\"matcher\":\"$SLUG\",\"target\":\"$TARGET\"}")
echo "$ROUTE" | jq -e '.id' >/dev/null || { log "FAIL create route: $ROUTE"; exit 1; }

# --- Stage 4: client config + run -----------------------------------------
log "STAGE client: writing $CFG and starting tunnel-client"
cat >"$CFG" <<EOF
worker_url = "ws://127.0.0.1:$PORT"
token = "$TOKEN"
[targets]
$TARGET = "127.0.0.1:$ORIGIN_PORT"
EOF
cargo run -q -p tunnel-client -- --config "$CFG" >"$CLOG" 2>&1 &
CLIENT_PID=$!

# Wait for the client to connect: the first HTTP round trip succeeds only once
# the client's WebSocket session is registered in the DO.
connected=""
for _ in $(seq 1 30); do
    if [ "$(curl -s -d hello "$BASE/$SLUG/echo" 2>/dev/null)" = "hello" ]; then
        connected=1
        break
    fi
    sleep 1
done
[ -n "$connected" ] || { log "FAIL client never connected / first round trip failed"; cat "$CLOG"; exit 1; }
log "STAGE client: connected"

# --- Stage 5: assertions --------------------------------------------------
fail=0

log "ASSERT HTTP echo"
HTTP_OUT=$(curl -s -d hello "$BASE/$SLUG/echo")
if [ "$HTTP_OUT" = "hello" ]; then
    log "  HTTP echo PASS (got '$HTTP_OUT')"
else
    log "  HTTP echo FAIL (got '$HTTP_OUT', want 'hello')"; fail=1
fi

log "ASSERT SSE"
SSE_OUT=$(curl -sN "$BASE/$SLUG/sse")
if echo "$SSE_OUT" | grep -q "event-2"; then
    log "  SSE PASS (contains event-2)"
else
    log "  SSE FAIL (output: $(echo "$SSE_OUT" | tr '\n' '|'))"; fail=1
fi

log "ASSERT WebSocket"
WS_OUT=$(node -e '
const url = process.argv[1];
const ws = new WebSocket(url);
const done = (code, msg) => { console.log(msg); process.exit(code); };
ws.onopen = () => ws.send("ping");
ws.onmessage = (e) => done(e.data === "echo:ping" ? 0 : 1, String(e.data));
ws.onerror = (e) => done(1, "wserror:" + (e && e.message ? e.message : "unknown"));
setTimeout(() => done(1, "timeout"), 10000);
' "ws://127.0.0.1:$PORT/$SLUG/ws") && WS_RC=0 || WS_RC=$?
if [ "$WS_RC" = "0" ] && [ "$WS_OUT" = "echo:ping" ]; then
    log "  WebSocket PASS (got '$WS_OUT')"
else
    log "  WebSocket FAIL (got '$WS_OUT', want 'echo:ping')"; fail=1
fi

# --- Stage 6: pool (two clients, one token) --------------------------------
ORIGIN2_PORT=9100
CFG2=/tmp/tunnel_e2e_2.toml
O2LOG=/tmp/e2e_origin2.log
C2LOG=/tmp/e2e_client2.log
STATUS_URL="$BASE/admin/clients/$CLIENT_ID/status"

log "STAGE pool: starting second origin on 127.0.0.1:$ORIGIN2_PORT"
DUMMY_ORIGIN_PORT=$ORIGIN2_PORT DUMMY_ORIGIN_ID=origin-b \
    cargo run -q -p tunnel-client --example dummy_origin >"$O2LOG" 2>&1 &
ORIGIN2_PID=$!
for _ in $(seq 1 60); do
    if curl -s -o /dev/null -d probe "http://127.0.0.1:$ORIGIN2_PORT/echo"; then break; fi
    sleep 1
done
curl -s -o /dev/null -d probe "http://127.0.0.1:$ORIGIN2_PORT/echo" \
    || { log "FAIL second origin never came up"; cat "$O2LOG"; exit 1; }

log "STAGE pool: starting second client on the same token"
cat >"$CFG2" <<EOF
worker_url = "ws://127.0.0.1:$PORT"
token = "$TOKEN"
[targets]
$TARGET = "127.0.0.1:$ORIGIN2_PORT"
EOF
cargo run -q -p tunnel-client -- --config "$CFG2" >"$C2LOG" 2>&1 &
CLIENT2_PID=$!

pool_ready=""
for _ in $(seq 1 30); do
    N=$(curl -s "$STATUS_URL" "${auth[@]}" | jq '.sockets | length')
    if [ "$N" = "2" ]; then pool_ready=1; break; fi
    sleep 1
done
[ -n "$pool_ready" ] || { log "FAIL pool never reached 2 sockets"; cat "$C2LOG"; exit 1; }
log "STAGE pool: 2 sockets connected"

log "ASSERT pool balancing (concurrent /slow requests land on different clients)"
curl -s "$BASE/$SLUG/slow" >/tmp/e2e_slow_a.out &
SLOW_A=$!
sleep 1  # let the first request register as in-flight before the second picks
curl -s "$BASE/$SLUG/slow" >/tmp/e2e_slow_b.out &
SLOW_B=$!
wait "$SLOW_A" "$SLOW_B" || true
BAL_A=$(cat /tmp/e2e_slow_a.out)
BAL_B=$(cat /tmp/e2e_slow_b.out)
if [ "$BAL_A" != "$BAL_B" ] \
    && echo "$BAL_A$BAL_B" | grep -q "slow:origin-a" \
    && echo "$BAL_A$BAL_B" | grep -q "slow:origin-b"; then
    log "  balancing PASS ($BAL_A / $BAL_B)"
else
    log "  balancing FAIL (got '$BAL_A' / '$BAL_B', want one from each origin)"; fail=1
fi

log "ASSERT fast failover (kill one client mid-request)"
curl -s -o /tmp/e2e_ff_a.body -w '%{http_code} %{time_total}' \
    "$BASE/$SLUG/slow" >/tmp/e2e_ff_a.meta &
FF_A=$!
sleep 1
curl -s -o /tmp/e2e_ff_b.body -w '%{http_code} %{time_total}' \
    "$BASE/$SLUG/slow" >/tmp/e2e_ff_b.meta &
FF_B=$!
sleep 1
kill "$CLIENT2_PID" 2>/dev/null || true
wait "$FF_A" "$FF_B" || true
SURVIVOR=""
FAILED_META=""
for f in a b; do
    CODE=$(cut -d' ' -f1 "/tmp/e2e_ff_$f.meta")
    if [ "$CODE" = "200" ]; then
        SURVIVOR=$(cat "/tmp/e2e_ff_$f.body")
    else
        FAILED_META=$(cat "/tmp/e2e_ff_$f.meta")
    fi
done
FAILED_CODE=${FAILED_META%% *}
FAILED_TIME=${FAILED_META##* }
# The dead socket's request must 502 well under both its own 6s response
# delay and the 30s head-timeout backstop; 5s is the generous ceiling.
if [ "$SURVIVOR" = "slow:origin-a" ] && [ "$FAILED_CODE" = "502" ] \
    && awk "BEGIN{exit !($FAILED_TIME < 5)}"; then
    log "  failover PASS (survivor '$SURVIVOR', dead request ${FAILED_CODE} in ${FAILED_TIME}s)"
else
    log "  failover FAIL (survivor '$SURVIVOR', failed request meta '$FAILED_META')"; fail=1
fi

log "ASSERT survivor serves new requests"
WHO=$(curl -s "$BASE/$SLUG/whoami")
if [ "$WHO" = "origin-a" ]; then
    log "  survivor PASS"
else
    log "  survivor FAIL (got '$WHO', want 'origin-a')"; fail=1
fi

log "ASSERT status drops to 1 socket"
one_left=""
for _ in $(seq 1 15); do
    N=$(curl -s "$STATUS_URL" "${auth[@]}" | jq '.sockets | length')
    if [ "$N" = "1" ]; then one_left=1; break; fi
    sleep 1
done
if [ -n "$one_left" ]; then
    log "  socket count PASS"
else
    log "  socket count FAIL (still $N sockets)"; fail=1
fi

if [ "$fail" != "0" ]; then
    log "E2E FAILED"
    exit 1
fi
log "ALL E2E CHECKS PASSED"
