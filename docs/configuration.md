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

The only CLI flag is `--config <path>`, which defaults to `tunnel.toml` in the working
directory. Setting `TUNNEL_TOKEN` in the environment overrides the `token` value from the
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
