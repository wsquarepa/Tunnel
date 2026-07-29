# Configuration

The client reads a TOML file:

```toml
worker_url = "wss://tunnel.example.workers.dev"
token      = "tnl_..." # or set TUNNEL_TOKEN in the environment

# named local targets this agent is willing to expose
[targets]
jupyter = "127.0.0.1:8888"
ollama  = "127.0.0.1:11434"
```

The admin panel only ever references targets by **name**. The binary resolves names to ports
from this file, so the edge can never make the client dial a port you did not list.

The client takes two CLI flags: `--config <path>` (defaults to `tunnel.toml`
in the working directory) and `--log <file>`, which additionally writes every
log event as one JSON object per line to the given file at trace verbosity,
independent of the terminal filter.

Terminal verbosity is controlled with `RUST_LOG` (default `info`). `debug`
narrates dial phases, handshake, keepalives, and per-stream lifecycle;
`trace` adds every control-socket frame (variant and size, never payloads)
and header dumps. Module targets work, e.g.
`RUST_LOG=info,tunnel_client::conn=trace`. The client also detects silently
dead links: if keepalive pings draw no inbound traffic for 90 seconds, the
connection is torn down and redialed automatically.

Setting `TUNNEL_TOKEN` in the environment overrides the `token` value from the
config file.

## Routing modes

- **Path-based** (default, works on `workers.dev`): `your-worker.workers.dev/jupyter/...`
  maps to the client's `jupyter` target. The route prefix is stripped, so the local app sees
  `/`. Apps that emit absolute URLs may misbehave under a path prefix; use a subdomain for
  those.
- **Subdomain-based** (requires a custom domain): `jupyter.tunnel.example.com` maps to the
  `jupyter` target, with the app served at root. It needs a wildcard DNS record, and
  `*.workers.dev` does **not** support wildcards.

## Admin panel

Single-secret login (the `ADMIN_SECRET` Worker secret). From the panel you can:

- create, disable, and delete clients, and view their one-time tokens at creation,
- assign and remove routes,
- see each client's live connection status, last-seen time, and a rolling log of recent
  requests (method, path, status, latency).
