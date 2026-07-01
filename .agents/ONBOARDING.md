# Tunnel: agent onboarding

This file is the shared briefing for coding agents. The canonical location for this file is
`.agents/ONBOARDING.md`. For further information, the user-facing docs live in [`docs/`](../docs).

## What this is

A persistent, self-hosted HTTP(S) and WebSocket tunnel on Cloudflare Workers and Durable
Objects. A native client opens one outbound WebSocket to its Durable Object; public requests
hit the Worker, get routed to that client's Durable Object, and are replayed against a
`localhost` port. The public URL is stable across reboots because the connection is outbound
and the Durable Object is addressed by a fixed client id.

Read [`docs/architecture.md`](../docs/architecture.md) for the full data flow.

## Workspace layout

```
crates/
  tunnel-protocol/   shared wire format: serde Frame enum encoded with postcard
  tunnel-worker/     workers-rs Worker (wasm32): router, admin API + panel, TunnelSession DO
  tunnel-client/     native Tokio binary: connects, proxies HTTP/SSE/WebSocket to local ports
xtask/               workspace automation (the test gate and the Cloudflare deploy)
docs/                user-facing documentation
```

Key files:

- `crates/tunnel-worker/src/lib.rs`: the Worker router (admin, tunnel connect, public routes).
- `crates/tunnel-worker/src/admin.rs`: admin API (login, client and route CRUD, CSRF).
- `crates/tunnel-worker/src/session.rs`: the `TunnelSession` Durable Object and request bridging.
- `crates/tunnel-worker/src/store.rs`: D1 registry (clients, routes).
- `crates/tunnel-worker/src/routing.rs`: pure host/path to route resolution.
- `crates/tunnel-client/src/conn.rs`: control-socket connection and frame dispatch.
- `crates/tunnel-protocol/src/frame.rs`: the `Frame` enum shared by both sides.

## Common commands

```sh
cargo xtask test     # the full gate: rustfmt check, clippy (host + wasm), tests. CI runs this.
cargo xtask deploy   # find/create the D1 database, inject its id, migrate, and deploy the Worker
```

End-to-end tests (need `npx wrangler`, Node 22+ with a global `WebSocket`, and a Rust toolchain):

```sh
crates/tunnel-client/tests/e2e.sh          # real client against a local `wrangler dev` Worker
crates/tunnel-worker/tests/live.sh         # regression suite against a deployed Worker (set WORKER_URL, ADMIN_SECRET)
```

The Worker only builds for `wasm32-unknown-unknown` (it is clippy-checked there, not
unit-tested); the client and protocol crates carry the host-run unit tests.

## Things that will bite you if you forget them

- **`wrangler.toml` keeps a placeholder `database_id` on purpose.** The real id is
  account-specific and never committed. `cargo xtask deploy` injects it for the deploy only and
  restores the placeholder afterward. Do not commit a real id.
- **The client owns its allowlist.** The Worker refers to targets by name only; the client
  resolves names to ports from its own config, so the edge can never make it dial an unlisted
  port. Preserve this boundary.
- **Privacy by construction.** The request log stores only method, path, status, latency, and
  target name. Never add headers or bodies to it.
- **Routes are public.** There is no per-route auth. See [`docs/security.md`](../docs/security.md),
  including the same-origin admin caution for path mode.
- **Durable Object in-memory state does not survive hibernation.** Anything held in the DO
  struct (pending requests, public WebSocket handles) is only valid while the DO is resident.

## Conventions

- Commits follow Conventional Commits (`type(scope): summary`).
- Prefer strict typing, pure functions, and small single-purpose functions.
- Keep changes minimal and match the surrounding style.

## Style: no em-dashes

Never use an em-dash (the `U+2014` character) anywhere: code, comments, string
literals, docs, commit messages, or UI copy. Rewrite the sentence instead,
using a comma, parentheses, a colon, or two sentences. Do not substitute a
regular hyphen or an en-dash for it; reword so the punctuation is not needed.

Scan the tree for violations (uses PCRE so this doc stays em-dash-free itself):

```sh
grep -rnP '\x{2014}' --include='*.rs' --include='*.ts' --include='*.tsx' \
  --include='*.css' --include='*.md' --include='*.html' --include='*.toml' \
  --include='*.sh' . | grep -vE '/(target|node_modules|\.git|dist)/'
```

A clean tree prints nothing. Any line it prints is a violation to reword.
