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

Multiple binaries sharing one token form a **pool**. Requests load-balance across them, and a
reboot self-heals: the dead connection drops and the fresh one joins, with no URL change.

The wire protocol lives in the `tunnel-protocol` crate (serde frames encoded with postcard)
and is shared by both the Worker and the client.
