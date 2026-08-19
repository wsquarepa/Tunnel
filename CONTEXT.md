# Tunnel

A self-hosted HTTP(S) and WebSocket tunnel: native clients hold outbound control sockets to a Durable Object on Cloudflare, and public requests are replayed against named local origins.

## Language

### Identity and connections

**Client**:
One admin-registered identity, addressed by a fixed client id and authenticated by one token. Several running binaries may share one client.
_Avoid_: Agent, tenant

**Control socket**:
One live WebSocket from a running binary to its client's Durable Object, carrying all tunnel frames for that binary.
_Avoid_: Connection, conn, link

**Pool**:
All live control sockets attached to one client.
_Avoid_: Cluster, group

### Targets and dispatch

**Target**:
A named local origin a binary may dial. The edge knows only the name; the port is resolved solely from the binary's own config.
_Avoid_: Service, backend, upstream, port

**Advertised targets**:
The set of target names one control socket declared in its Hello; it is the only set that socket will ever be dispatched for.
_Avoid_: Capabilities, served targets

**Capable socket**:
A live control socket whose advertised targets contain the requested target. A target with no capable socket is unreachable even when the pool is non-empty.
_Avoid_: Eligible socket, matching socket

**Effective targets**:
The subset of a binary's configured targets it both advertises and serves in one run; the whole config when no subset is given.
_Avoid_: Enabled targets, active targets
