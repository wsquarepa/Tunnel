# Development

Rust Cargo workspace:

```
crates/
  tunnel-protocol/   shared wire format (serde frames, postcard), used by both sides
  tunnel-worker/     workers-rs Worker: router, admin API/panel, Durable Object
  tunnel-client/     native Tokio binary
xtask/               workspace automation (test gate and Cloudflare deploy)
```

The full check suite runs through one command, which is also what CI runs:

```sh
cargo xtask test
```

It runs rustfmt, clippy on both the host and `wasm32-unknown-unknown`, and the tests. The
Worker compiles to `wasm32-unknown-unknown`; the client is a native binary.

## End-to-end tests

`crates/tunnel-client/tests/e2e.sh` runs the real client against a local `wrangler dev`
Worker and asserts that HTTP, SSE, and WebSocket all survive the round trip.

`crates/tunnel-worker/tests/live.sh` runs a regression suite against a deployed Worker over
HTTPS/WSS. Point it at your deployment:

```sh
WORKER_URL=https://<your-worker>.workers.dev ADMIN_SECRET=... crates/tunnel-worker/tests/live.sh
```

Both need `npx wrangler`, a `node` with a global `WebSocket` (Node 22+), and a Rust
toolchain.

## Deploying

`cargo xtask deploy` finds or creates the `tunnel` D1 database, injects its id into
`wrangler.toml` for the deploy only, applies the migrations, and deploys. See
[Deployment](deployment.md).
