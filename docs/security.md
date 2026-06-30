# Security

> [!CAUTION]
> **In path mode, only tunnel content you trust, and do not open an untrusted tunneled app in
> the same browser where you are signed into `/admin`.** In path mode (the default on
> `workers.dev`), tunneled apps share an origin with the admin panel
> (`your-worker.workers.dev/<slug>/…` vs `your-worker.workers.dev/admin/…`). `SameSite=Strict`
> and `HttpOnly` cookies do **not** restrain *same-origin* requests, so a malicious or
> compromised tunneled app's JavaScript can call the admin API with your logged-in session and
> create or delete clients and routes. The robust fix is to serve the admin panel on a separate
> origin (use subdomain mode with a dedicated admin host). This is acceptable for the intended
> single-operator, own-services use; if you must expose untrusted content, isolate the admin
> origin.

- **Tokens** are high-entropy, transmitted only over TLS (`wss`), and stored only as a
  SHA-256 hash. They are shown exactly once.
- **Admin auth** uses a single secret stored as a Wrangler secret, an HMAC-signed
  `HttpOnly` / `Secure` / `SameSite=Strict` session cookie, and constant-time comparison.
- **Client-owned allowlist:** the binary only dials ports named in its own config. A buggy or
  compromised control-plane cannot point it at arbitrary localhost ports.
- **Privacy by construction:** the request log records only method, path, status, latency,
  and target name. It never records headers or bodies.
- **Routes are public.** There is no per-route authentication; layer Cloudflare Access
  or a service in front if you need it.

## Limitations

- Multi-user accounts and roles.
- Per-route authentication.
- Full request/response body capture and replay.
- Strict single-connection / takeover mode. As of now, connections always pool.
- Automatic retry of failed requests across pool connections.

## Acceptable use

This project tunnels your own services through your own Cloudflare account. Cloudflare's
Self-Serve Subscription Agreement §2.2.1(j) prohibits using the Services "to provide a virtual
private network or other similar proxy services." Although it does seem to only target open or
anonymizing proxies and VPN relays (e.g. v2ray/VLESS forwarders that relay arbitrary third-party
traffic), it could very much also target a reverse proxy that exposes your own named backend
(i.e., this "product"). However (or 'Furthermore' if you read it that way), this service
operates very similarly to Cloudflare's own Cloudflare Tunnel product... so it's not entirely
clear whether this will sit well with them.

Please make sure you only tunnel your own services, never run an open relay or VPN, never forward
third-party traffic, and keep bandwidth modest to decrease the risk of ToS enforcement. Organizations
and high-bandwidth or multi-tenant use cases should use a commercial tunneling provider.

This is not legal advice. Under the LICENSE, the maintainer(s) are not liable for any resulting
penalty action against your account.

Please review Cloudflare's [Self-Serve Subscription Agreement](https://www.cloudflare.com/terms/)
for more information.
