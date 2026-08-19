# Architecture

```
 public request              ┌──────────┐   route lookup   ┌────────────────────────┐
 jupyter.example.com    ───▶ │  Worker  │ ───────────────▶ │  TunnelSession (DO)    │
 or example.com/jupyter      │ router + │                  │  · pool of client WS   │
                             │  admin   │ ◀─────────────── │  · load-balances       │
                             └────┬─────┘                  │  · request log (SQLite)│
                                  │                        └────────────┬───────────┘
                              ┌───▼────┐                                │
                              │   D1   │ clients, routes,       wss (control WS)
                              │registry│ token hashes                   │
                              └────────┘                   ┌────────────▼───────────┐
                                                           │  tunnel-client (Rust)  │
                                                           │  targets: jupyter=8888 │
                                                           └────────────┬───────────┘
                                                               localhost:8888, ...
```

1. The **client binary** opens one outbound WebSocket to its Durable Object and
   authenticates with a token. The connection survives NAT because it is outbound.
2. A **public request** hits the Worker, which resolves the host/path to a client and
   forwards it to that client's Durable Object.
3. The **Durable Object** multiplexes the request over the WebSocket to the binary, which
   replays it against the right `localhost:PORT` and streams the response back.

Multiple binaries sharing one token form a **pool**. Each request goes to the pool socket
with the fewest in-flight streams, so a long-lived SSE or WebSocket stream weighs against
the socket carrying it. When a socket dies, only its own in-flight requests fail
(immediately, instead of timing out), and a reboot self-heals: the dead connection drops
and the fresh one joins, with no URL change.

## Heterogeneous pool

The sockets in a pool need not be interchangeable. Each control socket declares its
**advertised targets** in its Hello, and a request for a target reaches only a **capable
socket**, one whose advertised targets contain that name, least loaded first. A non-empty
pool where nothing advertises the target answers `502 no capable socket for target`, which
is distinct from the `502 tunnel offline` an empty pool returns.

- One run is narrowed to a subset of the configured targets with `--targets a,b` or
  `TUNNEL_TARGETS=a,b`; the flag wins, and the subset is taken from the config's `[targets]`
  allowlist, so an unknown name or an empty result is a startup error.
- A socket's advertised set is fixed for its lifetime: restart the binary to change it.
- The edge still learns names only. Ports live in each binary's own config.

## Wire protocol

The wire protocol lives in the `tunnel-protocol` crate (serde frames encoded with postcard)
and is shared by both the Worker and the client.
