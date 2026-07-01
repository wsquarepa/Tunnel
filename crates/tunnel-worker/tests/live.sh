#!/usr/bin/env bash
# Real-Cloudflare regression suite for the bugs found in verification.
#
# Exercises the DEPLOYED worker over HTTPS/WSS (not `wrangler dev`), covering the
# control-plane HTTP contract plus a full data-plane round trip (which requires
# the client to speak `wss://`, i.e. TLS must be compiled into tunnel-client).
#
# Config via env (with sensible defaults for this project):
#   WORKER_URL    https base of the deployed worker (default the project worker)
#   ADMIN_SECRET  admin secret; falls back to the session scratchpad copy
#
# Exit 0 only if every assertion passes.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
WORKER_URL="${WORKER_URL:-https://tunnel.wsquarepa.workers.dev}"
WSS_URL="${WORKER_URL/https:/wss:}"
SECRET_FILE="${ADMIN_SECRET_FILE:-$ROOT/../1e782890-2661-43a1-bcf7-70412881b82c/scratchpad/admin_secret.txt}"
ADMIN_SECRET="${ADMIN_SECRET:-}"
if [ -z "$ADMIN_SECRET" ] && [ -f "$SECRET_FILE" ]; then
    ADMIN_SECRET="$(tr -d '\r\n' <"$SECRET_FILE")"
fi
[ -n "$ADMIN_SECRET" ] || { echo "FATAL: no ADMIN_SECRET (set ADMIN_SECRET or ADMIN_SECRET_FILE)"; exit 2; }

ORIGIN_PORT=9099
TAG="live-$$-$(date +%s)"
JAR="$(mktemp)"
OLOG="$(mktemp)"
CLOG="$(mktemp)"
CFG="$(mktemp --suffix=.toml)"
J3OUT="$(mktemp)"
CREATED_CLIENTS=()
CREATED_ROUTES=()

pass=0
fail=0
ok()   { echo "  PASS $1"; pass=$((pass+1)); }
bad()  { echo "  FAIL $1"; fail=$((fail+1)); }

auth=(-b "$JAR" -H "X-Tunnel-CSRF: 1" -H "Content-Type: application/json")

admin_login() {
    curl -s -c "$JAR" -o /dev/null -X POST "$WORKER_URL/admin/login" \
        -H 'Content-Type: application/json' -d "{\"secret\":\"$ADMIN_SECRET\"}"
}

cleanup() {
    for id in "${CREATED_ROUTES[@]:-}"; do
        [ -n "$id" ] && curl -s -o /dev/null "${auth[@]}" -X DELETE "$WORKER_URL/admin/routes/$id"
    done
    for id in "${CREATED_CLIENTS[@]:-}"; do
        [ -n "$id" ] && curl -s -o /dev/null "${auth[@]}" -X DELETE "$WORKER_URL/admin/clients/$id"
    done
    kill "${CLIENT_PID:-}" "${ORIGIN_PID:-}" 2>/dev/null || true
    rm -f "$JAR" "$OLOG" "$CLOG" "$CFG" "$J3OUT"
}
trap cleanup EXIT

# code() runs curl and prints only the HTTP status.
code() { curl -s -o /dev/null -w '%{http_code}' "$@"; }
# is4xx() true when the first arg is a 4xx status.
is4xx() { [[ "$1" =~ ^4[0-9][0-9]$ ]]; }

admin_login
grep -q tunnel_session "$JAR" || { echo "FATAL: admin login failed"; exit 2; }

echo "== Control plane =="

# A2: /admin/ must serve the panel, not fall through to the tunnel 404.
body="$(curl -s "$WORKER_URL/admin/")"
if echo "$body" | grep -qi '<!doctype html'; then ok "A2 /admin/ serves panel"; else bad "A2 /admin/ -> '$body'"; fi

# B8: malformed / missing-field login body must be 4xx, not a 500 that leaks internals.
c="$(code -X POST "$WORKER_URL/admin/login" -H 'Content-Type: application/json' -d '{}')"
is4xx "$c" && ok "B8 login {} -> $c" || bad "B8 login {} -> $c (want 4xx)"
c="$(code -X POST "$WORKER_URL/admin/login" -H 'Content-Type: application/json' -d 'not json')"
is4xx "$c" && ok "B8 login bad-json -> $c" || bad "B8 login bad-json -> $c (want 4xx)"

# D2: client list must not expose token_hash.
cc="$(curl -s "${auth[@]}" -X POST "$WORKER_URL/admin/clients" -d "{\"name\":\"$TAG-d2\"}")"
cid="$(echo "$cc" | jq -r '.id')"; CREATED_CLIENTS+=("$cid")
cctok="$(echo "$cc" | jq -r '.token')"
keys="$(curl -s "${auth[@]}" "$WORKER_URL/admin/clients" | jq -r '.[].token_hash // empty' | head -1)"
[ -z "$keys" ] && ok "D2 no token_hash in list" || bad "D2 token_hash leaked in /admin/clients"

# Connect with a valid token but no Upgrade header (e.g. a browser hitting the
# URL) must be a clean 426, not a 500 that leaks a TypeError.
c="$(code "$WORKER_URL/_tunnel/connect?token=$cctok")"
[ "$c" = "426" ] && ok "connect no-upgrade -> 426" || bad "connect no-upgrade -> $c (want 426)"

# D7: missing name -> 4xx; empty name -> 4xx.
c="$(code "${auth[@]}" -X POST "$WORKER_URL/admin/clients" -d '{}')"
is4xx "$c" && ok "D7 client {} -> $c" || bad "D7 client {} -> $c (want 4xx)"
c="$(code "${auth[@]}" -X POST "$WORKER_URL/admin/clients" -d '{"name":""}')"
is4xx "$c" && ok "D7 client empty-name -> $c" || bad "D7 client empty-name -> $c (want 4xx)"

# E9: malformed / missing-field route body -> 4xx.
c="$(code "${auth[@]}" -X POST "$WORKER_URL/admin/routes" -d 'not json')"
is4xx "$c" && ok "E9 route bad-json -> $c" || bad "E9 route bad-json -> $c (want 4xx)"
c="$(code "${auth[@]}" -X POST "$WORKER_URL/admin/routes" -d '{"kind":"path"}')"
is4xx "$c" && ok "E9 route missing-fields -> $c" || bad "E9 route missing-fields -> $c (want 4xx)"

# E7: duplicate (kind,matcher) -> clean 4xx, and the body must not leak a D1 stack trace.
r1="$(curl -s "${auth[@]}" -X POST "$WORKER_URL/admin/routes" \
    -d "{\"client_id\":\"$cid\",\"kind\":\"path\",\"matcher\":\"$TAG\",\"target\":\"demo\"}")"
rid="$(echo "$r1" | jq -r '.id')"; CREATED_ROUTES+=("$rid")
dup_status="$(code "${auth[@]}" -X POST "$WORKER_URL/admin/routes" \
    -d "{\"client_id\":\"$cid\",\"kind\":\"path\",\"matcher\":\"$TAG\",\"target\":\"demo\"}")"
dup_body="$(curl -s "${auth[@]}" -X POST "$WORKER_URL/admin/routes" \
    -d "{\"client_id\":\"$cid\",\"kind\":\"path\",\"matcher\":\"$TAG\",\"target\":\"demo\"}")"
if is4xx "$dup_status" && ! echo "$dup_body" | grep -qiE 'D1Error|constraint failed|at .*\.js'; then
    ok "E7 duplicate route -> $dup_status, no leak"
else
    bad "E7 duplicate route -> $dup_status body='$dup_body'"
fi

# G6: root path -> 404 "no such tunnel" (consistent worker message).
gbody="$(curl -s "$WORKER_URL/")"
[ "$gbody" = "no such tunnel" ] && ok "G6 / -> 'no such tunnel'" || bad "G6 / -> '$gbody'"

echo "== Data plane (wss round trip) =="
if ! command -v cargo >/dev/null; then
    echo "  SKIP data plane (cargo unavailable)"
else
    cargo build -q -p tunnel-client --example dummy_origin 2>/dev/null
    cargo build -q -p tunnel-client 2>/dev/null
    ( cd "$ROOT" && cargo run -q -p tunnel-client --example dummy_origin ) >"$OLOG" 2>&1 &
    ORIGIN_PID=$!
    for _ in $(seq 1 60); do
        [ "$(curl -s -d probe "http://127.0.0.1:$ORIGIN_PORT/echo" 2>/dev/null)" = "probe" ] && break
        sleep 1
    done

    r2="$(curl -s "${auth[@]}" -X POST "$WORKER_URL/admin/routes" \
        -d "{\"client_id\":\"$cid\",\"kind\":\"path\",\"matcher\":\"$TAG-dp\",\"target\":\"demo\"}")"
    rid2="$(echo "$r2" | jq -r '.id')"; CREATED_ROUTES+=("$rid2")
    # A fresh token: create a dedicated client so its token is known.
    dpc="$(curl -s "${auth[@]}" -X POST "$WORKER_URL/admin/clients" -d "{\"name\":\"$TAG-dp\"}")"
    dpid="$(echo "$dpc" | jq -r '.id')"; CREATED_CLIENTS+=("$dpid")
    dptok="$(echo "$dpc" | jq -r '.token')"
    # Point the data-plane route at the dedicated client.
    curl -s -o /dev/null "${auth[@]}" -X DELETE "$WORKER_URL/admin/routes/$rid2"
    r3="$(curl -s "${auth[@]}" -X POST "$WORKER_URL/admin/routes" \
        -d "{\"client_id\":\"$dpid\",\"kind\":\"path\",\"matcher\":\"$TAG-dp\",\"target\":\"demo\"}")"
    CREATED_ROUTES+=("$(echo "$r3" | jq -r '.id')")

    cat >"$CFG" <<EOF
worker_url = "$WSS_URL"
token = "$dptok"
[targets]
demo = "127.0.0.1:$ORIGIN_PORT"
EOF
    ( cd "$ROOT" && cargo run -q -p tunnel-client -- --config "$CFG" ) >"$CLOG" 2>&1 &
    CLIENT_PID=$!

    connected=""
    for _ in $(seq 1 30); do
        if [ "$(curl -s -d hello "$WORKER_URL/$TAG-dp/echo" 2>/dev/null)" = "hello" ]; then connected=1; break; fi
        sleep 1
    done
    if [ -n "$connected" ]; then
        ok "F1/G1 wss connect + HTTP echo"
        sse="$(curl -sN "$WORKER_URL/$TAG-dp/sse")"
        echo "$sse" | grep -q event-2 && ok "H1 SSE stream" || bad "H1 SSE -> $(echo "$sse" | tr '\n' '|')"
        ws="$(node -e '
          const ws = new WebSocket(process.argv[1]);
          const done=(c,m)=>{console.log(m);process.exit(c)};
          ws.onopen=()=>ws.send("ping");
          ws.onmessage=e=>done(e.data==="echo:ping"?0:1,String(e.data));
          ws.onerror=()=>done(1,"wserror");
          setTimeout(()=>done(1,"timeout"),10000);
        ' "$WSS_URL/$TAG-dp/ws" 2>/dev/null)"
        [ "$ws" = "echo:ping" ] && ok "I1 WebSocket echo" || bad "I1 WS -> '$ws'"

        # G7: a custom header reaches the origin; a forged x-tunnel-* is stripped.
        hdr="$(curl -s "$WORKER_URL/$TAG-dp/headers" -H 'X-Probe: abc123' -H 'X-Tunnel-Target: evil')"
        if echo "$hdr" | jq -e '.["x-probe"]=="abc123"' >/dev/null 2>&1 \
           && ! echo "$hdr" | jq -e 'has("x-tunnel-target")' >/dev/null 2>&1; then
            ok "G7 custom header forwarded, x-tunnel-* stripped"
        else bad "G7 headers -> $hdr"; fi

        # G8: a hanging origin trips the edge head-timeout backstop (504) within ~30s.
        g8="$(code -m 45 "$WORKER_URL/$TAG-dp/hang")"
        [ "$g8" = "504" ] && ok "G8 hang -> 504" || bad "G8 hang -> $g8 (want 504)"

        # I2: a public-initiated close must reach CLOSED (onclose fires); a socket
        # stuck in CLOSING never fires onclose and hits the timeout branch.
        i2="$(node -e '
          const ws=new WebSocket(process.argv[1]);
          ws.onopen=()=>ws.send("ping");
          ws.onmessage=()=>ws.close(1000);
          ws.onclose=()=>{console.log("closed");process.exit(0)};
          ws.onerror=()=>{console.log("error");process.exit(1)};
          setTimeout(()=>{console.log("stuck:"+ws.readyState);process.exit(1)},8000);
        ' "$WSS_URL/$TAG-dp/ws" 2>/dev/null)"
        [ "$i2" = "closed" ] && ok "I2 public close completes" || bad "I2 close -> '$i2'"

        # J3 (last): open a public WS, drop the only client; the public WS must be
        # closed by the DO, not left hanging. This kills the client, so run it last.
        node -e '
          const ws=new WebSocket(process.argv[1]);
          ws.onclose=()=>{console.log("closed");process.exit(0)};
          setTimeout(()=>{console.log("hung");process.exit(1)},20000);
        ' "$WSS_URL/$TAG-dp/ws" >"$J3OUT" 2>/dev/null &
        j3pid=$!
        sleep 3
        kill -9 "$CLIENT_PID" 2>/dev/null; CLIENT_PID=""
        wait "$j3pid" 2>/dev/null
        [ "$(cat "$J3OUT")" = "closed" ] && ok "J3 public WS closed on last-client drop" || bad "J3 -> $(cat "$J3OUT")"
    else
        bad "F1/G1 client never connected over wss (client log: $(tail -1 "$CLOG"))"
        bad "H1 SSE (blocked by F1)"
        bad "I1 WebSocket (blocked by F1)"
    fi
fi

echo "== Summary: $pass passed, $fail failed =="
[ "$fail" -eq 0 ]
