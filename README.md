# Tunnel

A persistent, self-hosted HTTP(S) and WebSocket tunnel built on Cloudflare Workers and
Durable Objects. It gives a service behind NAT (a GPU box, a home server, a laptop) a
**stable public URL that survives reboots**, without handing out a fresh URL every time and
without requiring visitors to authenticate to your infrastructure.

> [!WARNING]
> A route you create is **public on the internet**. Anyone with the URL can reach the
> service behind it. There is no per-route authentication, so only expose things you intend to
> be public, or put Cloudflare Access in front. See [Security](docs/security.md).

> [!NOTE]
> This exposes your own backend at a stable URL by reverse-proxying it through your own
> Cloudflare Worker, the same pattern as Cloudflare Tunnel. Do **not** use it as a VPN, an open
> proxy, or to relay third-party traffic: Cloudflare's terms (§2.2.1(j)) prohibit providing "a
> virtual private network or other similar proxy services," and accounts running open proxies
> risk suspension. Organizations, or anyone needing multi-tenant features, per-route auth, or
> heavy bandwidth, should use a paid service such as ngrok. See
> [Acceptable use](docs/security.md#acceptable-use).

## Getting started

### 1. Deploy the Worker

```sh
# create the D1 database, migrate, and deploy
cargo xtask deploy
# then, set the panel login secret
cd crates/tunnel-worker && npx wrangler secret put ADMIN_SECRET
```

### 2. Create a client and copy its token

Open `https://<your-worker>.workers.dev/admin`, log in with `ADMIN_SECRET`, and create a
client. **Copy the token; it is shown only once.** Then add a route mapping a public path (or
subdomain) to one of that client's named targets.

### 3. Install and run the client on your server

Install the latest client binary (Linux and macOS):

```sh
curl -fsSL https://github.com/wsquarepa/Tunnel/raw/master/install.sh | bash
```

Or build it from source: `cargo build --release -p tunnel-client`. Windows users can download
the `.exe` from the [nightly release](https://github.com/wsquarepa/Tunnel/releases/tag/nightly).

Then point it at your Worker:

```toml
# tunnel.toml
worker_url = "wss://<your-worker>.workers.dev"
token      = "tnl_..."

[targets]
jupyter = "127.0.0.1:8888" # <-- change for your own service, or add more targets.
```

```sh
tunnel-client --config tunnel.toml
```

Your service is now reachable at `https://<your-worker>.workers.dev/jupyter/`.

## Documentation

- [Architecture](docs/architecture.md): how the Worker, Durable Object, and client fit together.
- [Deployment](docs/deployment.md): requirements, deploying, and running as a systemd service.
- [Configuration](docs/configuration.md): the client config file, routing modes, and the admin panel.
- [Security](docs/security.md): the trust model, limitations, and acceptable use.
- [Development](docs/development.md): workspace layout, tests, and the `cargo xtask` workflow.

## License

Licensed under the GNU General Public License v3.0. See [LICENSE](LICENSE).
