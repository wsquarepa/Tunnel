# Deployment

## Requirements

- A free [Cloudflare Workers](https://workers.cloudflare.com/) account. Durable Objects and
  D1 are available on the free plan.
- [`wrangler`](https://developers.cloudflare.com/workers/wrangler/) for deploying the Worker.
- A Rust toolchain to build the client, or a prebuilt binary from Releases.
- Optionally, a domain on Cloudflare for subdomain-per-service routing. Without one,
  everything works over `*.workers.dev` using path-based routing.

## Deploy the Worker

The `xtask` helper finds or creates the `tunnel` D1 database, injects its id into
`wrangler.toml` for the deploy only, applies the migrations, and deploys:

```sh
cargo xtask deploy
(cd crates/tunnel-worker && npx wrangler secret put ADMIN_SECRET)
```

To do the same by hand, create the database, copy the printed `database_id` into
`crates/tunnel-worker/wrangler.toml`, then:

```sh
wrangler d1 create tunnel
wrangler d1 migrations apply tunnel
wrangler secret put ADMIN_SECRET
wrangler deploy
```

## Install the client

The install script pulls the latest prebuilt binary from the rolling
[nightly release](https://github.com/wsquarepa/Tunnel/releases/tag/nightly) (Linux and macOS):

```sh
curl -fsSL https://github.com/wsquarepa/Tunnel/raw/master/install.sh | bash
```

It picks the right target for your OS and architecture (a static musl build on x86_64 Linux, so
it runs anywhere). On a terminal it asks whether to install system-wide (`/usr/local/bin`, using
sudo) or for just your user (`~/.local/bin`, no root). Windows users can download the `.exe` from
the nightly release directly.

For unattended installs, pass flags instead of answering the prompt:

```sh
curl -fsSL https://github.com/wsquarepa/Tunnel/raw/master/install.sh | bash -s -- --user
curl -fsSL https://github.com/wsquarepa/Tunnel/raw/master/install.sh | bash -s -- --system -y
```

`--user`, `--system`, `--dest DIR` (or `DEST=DIR`), and `-y` are supported; run the script with
`--help` for details.

To build from source instead:

```sh
cargo build --release -p tunnel-client
sudo cp target/release/tunnel-client /usr/local/bin/
```

## Run the client

```sh
tunnel-client --config tunnel.toml
```

See [Configuration](configuration.md) for the config file format.

## Run as a service (systemd)

A ready-to-use unit is provided at `crates/tunnel-client/dist/tunnel-client.service`. It
auto-starts the binary on boot so your URLs stay stable across reboots.

```sh
# 1. install the binary (download a release or build it)
curl -fsSL https://github.com/wsquarepa/Tunnel/raw/master/install.sh | bash
#   or: cargo build --release -p tunnel-client && sudo cp target/release/tunnel-client /usr/local/bin/

# 2. put your config where the unit expects it
sudo install -Dm600 tunnel.toml /etc/tunnel/tunnel.toml

# 3. install and start the unit
sudo cp crates/tunnel-client/dist/tunnel-client.service /etc/systemd/system/
sudo systemctl enable --now tunnel-client
```

The token can live in `/etc/tunnel/tunnel.toml` or be supplied to the unit via an
`Environment=TUNNEL_TOKEN=tnl_...` line (uncomment it in the unit), which overrides the file
token.
