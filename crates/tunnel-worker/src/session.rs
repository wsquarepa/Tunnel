use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use futures::channel::{mpsc, oneshot};
use futures::future::Either;
use futures::StreamExt;
use gloo_timers::future::TimeoutFuture;
use tunnel_protocol::{body_chunks, decode, encode, is_compatible, Frame};
use worker::*;

use crate::session_helpers::parse_bearer;
use crate::{routing, store, token};

/// Upstream head must arrive within this budget or the request fails with 504.
const HEAD_TIMEOUT_MS: u32 = 30_000;

/// Public entrypoint for the client control-plane WebSocket upgrade.
///
/// Authenticates the connecting binary by its `tnl_` token (from an
/// `Authorization: Bearer` header or `?token=` query) and, on success, forwards
/// the original request verbatim to the client's Durable Object. Forwarding the
/// unmodified request is what preserves the `Sec-WebSocket-*` upgrade headers.
pub async fn route_connect(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let token_str = req
        .headers()
        .get("Authorization")?
        .as_deref()
        .and_then(parse_bearer)
        .map(str::to_string)
        .or_else(|| {
            req.url().ok().and_then(|u| {
                u.query_pairs()
                    .find(|(k, _)| k == "token")
                    .map(|(_, v)| v.into_owned())
            })
        });
    let Some(token_str) = token_str else {
        console_warn!("event=auth_rejected reason=missing_token");
        return Response::error("missing token", 401);
    };

    let db = ctx.env.d1("DB")?;
    let hash = token::sha256_hex(&token_str);
    let Some(client) = store::find_client_by_token_hash(&db, &hash).await? else {
        console_warn!("event=auth_rejected reason=invalid_token");
        return Response::error("invalid token", 401);
    };

    // A valid token on a non-Upgrade request (e.g. a browser opening the URL)
    // would make the DO return a WebSocket without an upgrade, surfacing a 500.
    // Reject it cleanly before forwarding.
    let is_upgrade = req
        .headers()
        .get("Upgrade")?
        .is_some_and(|u| u.eq_ignore_ascii_case("websocket"));
    if !is_upgrade {
        console_warn!(
            "event=auth_rejected reason=not_upgrade client={}",
            client.id
        );
        return Response::error("expected websocket upgrade", 426);
    }

    let stub = ctx
        .durable_object("TUNNEL")?
        .id_from_name(&client.id)?
        .get_stub()?;
    console_log!(
        "event=client_connected client={} name={}",
        client.id,
        client.name
    );
    stub.fetch_with_request(req).await
}

/// Public entrypoint for proxied end-user traffic.
///
/// Resolves `(host, path)` to a route, then repackages the request as an internal
/// DO request carrying the routing metadata in `X-Tunnel-*` headers plus the body,
/// and returns the DO's streamed response. `404` when no route matches.
pub async fn route_public(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let host = url.host_str().unwrap_or_default().to_string();
    // APEX_HOST may be unset -> path mode only.
    let apex = ctx.var("APEX_HOST").ok().map(|v| v.to_string());
    let Some(resolved) = routing::resolve(&host, url.path(), apex.as_deref()) else {
        console_warn!("event=route_miss host={} path={}", host, url.path());
        return Response::error("no such tunnel", 404);
    };

    let db = ctx.env.d1("DB")?;
    let Some(route) = store::find_route(&db, resolved.kind, &resolved.matcher).await? else {
        console_warn!("event=route_miss host={} path={}", host, url.path());
        return Response::error("no such tunnel", 404);
    };

    let stub = ctx
        .durable_object("TUNNEL")?
        .id_from_name(&route.client_id)?
        .get_stub()?;

    let method = req.method().to_string();
    // A public WebSocket upgrade is bridged separately by the DO; flag it so the
    // DO takes the `handle_ws` branch instead of the plain request path.
    let is_ws_upgrade = req
        .headers()
        .get("Upgrade")?
        .is_some_and(|u| u.eq_ignore_ascii_case("websocket"));
    let headers = req.headers().clone();
    // Strip any client-supplied routing headers before setting the server's own;
    // a public caller must never be able to forge X-Tunnel-* (e.g. spoof
    // `X-Tunnel-Upgrade: websocket` to reach the DO's `handle_ws` branch).
    headers.delete("X-Tunnel-Target")?;
    headers.delete("X-Tunnel-Path")?;
    headers.delete("X-Tunnel-Method")?;
    headers.delete("X-Tunnel-Upgrade")?;
    headers.set("X-Tunnel-Target", &route.target)?;
    headers.set("X-Tunnel-Path", &resolved.local_path)?;
    headers.set("X-Tunnel-Method", &method)?;
    headers.set(
        "X-Tunnel-Upgrade",
        if is_ws_upgrade { "websocket" } else { "" },
    )?;
    let body = req.bytes().await.unwrap_or_default();

    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(js_sys::Uint8Array::from(body.as_slice()).into()));
    let do_req = Request::new_with_init("http://do/req", &init)?;
    stub.fetch_with_request(do_req).await
}

/// One row of the DO's request log, as returned by the `/status` endpoint.
#[derive(serde::Serialize, serde::Deserialize)]
struct RequestLogRow {
    ts: i64,
    method: String,
    path: String,
    status: i64,
    latency_ms: i64,
    target: String,
}

/// Head of a response: status + headers, delivered once per stream.
pub struct RespHeadInfo {
    pub status: u16,
    pub headers: Vec<(String, String)>,
}

/// Per-stream correlation state held while a request is in flight.
struct Pending {
    head: Option<oneshot::Sender<std::result::Result<RespHeadInfo, String>>>,
    body: mpsc::UnboundedSender<std::result::Result<Vec<u8>, String>>,
}

#[durable_object]
pub struct TunnelSession {
    state: State,
    next_stream: Rc<RefCell<u32>>,
    pending: Rc<RefCell<HashMap<u32, Pending>>>,
    /// Public-facing WebSocket sockets, keyed by tunnel stream id. Each entry is
    /// the server half of a `WebSocketPair` we hand to a public WS caller; frames
    /// arriving from the client over the control channel are routed back to it.
    ws_streams: Rc<RefCell<HashMap<u32, WebSocket>>>,
}

impl TunnelSession {
    fn alloc_stream(&self) -> u32 {
        let mut n = self.next_stream.borrow_mut();
        *n += 1;
        *n
    }

    /// Pick the first live socket in the pool (round-robin comes in a later task).
    fn pick_socket(&self) -> Option<WebSocket> {
        self.state.get_websockets().into_iter().next()
    }

    fn send_frame(ws: &WebSocket, frame: &Frame) -> Result<()> {
        let bytes = encode(frame).map_err(|e| Error::RustError(e.to_string()))?;
        ws.send_with_bytes(bytes)
    }
}

impl DurableObject for TunnelSession {
    fn new(state: State, _env: Env) -> Self {
        Self {
            state,
            next_stream: Rc::new(RefCell::new(0)),
            pending: Rc::new(RefCell::new(HashMap::new())),
            ws_streams: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        // The connect route forwards the public request verbatim (to preserve the
        // WebSocket upgrade headers), so its path is the public `/_tunnel/connect`;
        // the `/req` route is synthesized as `http://do/req`. Match by suffix to
        // accept both without rebuilding the upgrade request.
        let path = req.path();
        if path.ends_with("/connect") {
            self.handle_connect().await
        } else if path.ends_with("/req") {
            if req.headers().get("X-Tunnel-Upgrade")?.as_deref() == Some("websocket") {
                self.handle_ws(req).await
            } else {
                self.handle_req(req).await
            }
        } else if path.ends_with("/status") {
            self.handle_status().await
        } else {
            Response::error("not found", 404)
        }
    }

    async fn websocket_message(
        &self,
        ws: WebSocket,
        message: WebSocketIncomingMessage,
    ) -> Result<()> {
        let bytes = match message {
            WebSocketIncomingMessage::Binary(b) => b,
            // Control frames never arrive as text in this protocol; ignore.
            WebSocketIncomingMessage::String(_) => return Ok(()),
        };
        let frame = decode(&bytes).map_err(|e| {
            console_error!("event=session_error kind=decode");
            Error::RustError(e.to_string())
        })?;
        // The token was already verified at connect; the DO only checks the
        // protocol version and acknowledges the handshake.
        if let Frame::Hello { proto_version, .. } = &frame {
            if !is_compatible(*proto_version) {
                console_warn!(
                    "event=auth_rejected reason=proto_mismatch proto={}",
                    proto_version
                );
                ws.close(Some(1008u16), Some("incompatible protocol version"))?;
                return Ok(());
            }
            // A per-connection session id is assigned in a later task; a fixed
            // value is sufficient for the single-socket handshake today.
            Self::send_frame(
                &ws,
                &Frame::HelloAck {
                    session_id: 1,
                    server_version: env!("CARGO_PKG_VERSION").to_string(),
                },
            )?;
            return Ok(());
        }
        self.on_frame(frame);
        Ok(())
    }

    async fn websocket_close(
        &self,
        _ws: WebSocket,
        code: usize,
        _reason: String,
        clean: bool,
    ) -> Result<()> {
        console_log!("event=client_disconnected code={code} clean={clean}");
        // If this was the last socket, everything the tunnel was relaying is now
        // dead: fail each in-flight HTTP request (`pending`) by erroring its head
        // oneshot (-> 502) and dropping the Pending body senders to end response
        // streams, and close each active public WebSocket peer (`ws_streams`) with
        // 1001 so browsers get a close frame instead of hanging until their own
        // idle/TCP timeout.
        // Ceiling: for a multi-socket pool we can only tell the pool is non-empty,
        // not which streams belonged to the dead socket, so a partial-pool close
        // leaves its streams to the 504 head timeout as the backstop. Per-socket
        // stream tracking is a later refinement.
        //
        // The socket being closed is still returned by `get_websockets()` during
        // its own close callback, so "this was the last one" means exactly one
        // remains (the closing socket itself), not zero.
        if self.state.get_websockets().len() <= 1 {
            for (_, mut p) in self.pending.borrow_mut().drain() {
                if let Some(tx) = p.head.take() {
                    let _ = tx.send(Err("tunnel disconnected".to_string()));
                }
            }
            for (_, public_ws) in self.ws_streams.borrow_mut().drain() {
                let _ = public_ws.close(Some(1001u16), Some("tunnel disconnected"));
            }
        }
        Ok(())
    }
}

impl TunnelSession {
    async fn handle_connect(&self) -> Result<Response> {
        let pair = WebSocketPair::new()?;
        self.state.accept_web_socket(&pair.server);
        Response::from_websocket(pair.client)
    }

    async fn handle_req(&self, mut req: Request) -> Result<Response> {
        let Some(ws) = self.pick_socket() else {
            return Response::error("tunnel offline", 502);
        };
        let target = req.headers().get("X-Tunnel-Target")?.unwrap_or_default();
        let path = req
            .headers()
            .get("X-Tunnel-Path")?
            .unwrap_or_else(|| "/".to_string());
        let method = req
            .headers()
            .get("X-Tunnel-Method")?
            .unwrap_or_else(|| "GET".to_string());

        // Forward the caller's headers minus hop-by-hop and our internal routing
        // headers; the tunnel is a fresh hop so end-to-end headers only.
        let mut fwd_headers: Vec<(String, String)> = Vec::new();
        for (k, v) in req.headers().entries() {
            let lk = k.to_ascii_lowercase();
            if matches!(
                lk.as_str(),
                "connection" | "keep-alive" | "transfer-encoding" | "upgrade"
            ) || lk.starts_with("x-tunnel-")
            {
                continue;
            }
            fwd_headers.push((k, v));
        }

        let body = req.bytes().await.unwrap_or_default();
        let has_body = !body.is_empty();

        let stream = self.alloc_stream();
        let (head_tx, head_rx) = oneshot::channel();
        let (body_tx, body_rx) = mpsc::unbounded();
        self.pending.borrow_mut().insert(
            stream,
            Pending {
                head: Some(head_tx),
                body: body_tx,
            },
        );

        let started = Date::now().as_millis() as i64;

        Self::send_frame(
            &ws,
            &Frame::ReqHead {
                stream,
                target: target.clone(),
                method: method.clone(),
                path: path.clone(),
                headers: fwd_headers,
                has_body,
            },
        )?;
        for chunk in body_chunks(&body) {
            Self::send_frame(
                &ws,
                &Frame::ReqBody {
                    stream,
                    data: chunk.to_vec(),
                },
            )?;
        }
        Self::send_frame(&ws, &Frame::ReqEnd { stream })?;

        let head = futures::future::select(head_rx, TimeoutFuture::new(HEAD_TIMEOUT_MS)).await;
        let result = match head {
            Either::Left((Ok(Ok(h)), _)) => {
                self.log_request(&method, &path, h.status, started, &target);
                let headers = Headers::new();
                for (k, v) in &h.headers {
                    headers.set(k, v)?;
                }
                let body = body_rx.map(|chunk| chunk.map_err(Error::RustError));
                // The client relays the upstream body and its Content-Encoding
                // verbatim (reqwest has no decompression features), so the body is
                // already encoded. Manual mode keeps the runtime from compressing
                // it a second time; Automatic would double-encode and the browser
                // would decode to still-compressed bytes.
                Response::from_stream(body)?
                    .with_status(h.status)
                    .with_headers(headers)
                    .with_encode_body(EncodeBody::Manual)
            }
            Either::Left((Ok(Err(msg)), _)) => {
                self.log_request(&method, &path, 502, started, &target);
                Response::error(format!("upstream error: {msg}"), 502)?
            }
            Either::Left((Err(_), _)) => {
                self.log_request(&method, &path, 502, started, &target);
                Response::error("tunnel closed", 502)?
            }
            Either::Right(_) => {
                // Head budget exhausted: drop the correlation entry so a late
                // RespHead is ignored, and surface a gateway timeout.
                self.pending.borrow_mut().remove(&stream);
                self.log_request(&method, &path, 504, started, &target);
                Response::error("upstream timeout", 504)?
            }
        };
        Ok(result)
    }

    /// Bridge a public WebSocket upgrade to the client over the control channel.
    ///
    /// Opens a second `WebSocketPair` toward the public caller, tells the client to
    /// dial its local WS (`WsOpen`), and pumps public→client messages as `WsData`
    /// until either side closes. Client→public messages are routed by `on_frame`.
    async fn handle_ws(&self, req: Request) -> Result<Response> {
        let Some(client_ws) = self.pick_socket() else {
            return Response::error("tunnel offline", 502);
        };
        let target = req.headers().get("X-Tunnel-Target")?.unwrap_or_default();
        let path = req
            .headers()
            .get("X-Tunnel-Path")?
            .unwrap_or_else(|| "/".to_string());

        let stream = self.alloc_stream();
        let WebSocketPair { client, server } = WebSocketPair::new()?;
        server.accept()?;
        self.ws_streams.borrow_mut().insert(stream, server.clone());

        Self::send_frame(
            &client_ws,
            &Frame::WsOpen {
                stream,
                target,
                path,
                headers: vec![],
            },
        )?;

        // Pump public → client. `spawn_local` because the DO fetch returns the
        // upgrade response immediately while the socket stays live.
        let ws_streams = self.ws_streams.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let mut events = match server.events() {
                Ok(events) => events,
                Err(_) => return,
            };
            while let Some(Ok(event)) = events.next().await {
                match event {
                    WebsocketEvent::Message(m) => {
                        let (binary, data) = match m.bytes() {
                            Some(bytes) => (true, bytes),
                            None => (false, m.text().unwrap_or_default().into_bytes()),
                        };
                        let _ = Self::send_frame(
                            &client_ws,
                            &Frame::WsData {
                                stream,
                                binary,
                                data,
                            },
                        );
                    }
                    WebsocketEvent::Close(event) => {
                        let _ = Self::send_frame(
                            &client_ws,
                            &Frame::WsClose {
                                stream,
                                code: event.code(),
                                reason: event.reason(),
                            },
                        );
                        // Complete the closing handshake toward the public caller;
                        // without our reciprocal close frame the browser socket is
                        // stuck in CLOSING until its own idle timeout.
                        let _ = server.close(Some(event.code()), Some(event.reason().as_str()));
                        ws_streams.borrow_mut().remove(&stream);
                        break;
                    }
                }
            }
        });

        Response::from_websocket(client)
    }

    /// Report live connection count and the recent request log for this client's DO.
    async fn handle_status(&self) -> Result<Response> {
        let sql = self.state.storage().sql();
        let _ = sql.exec(
            "CREATE TABLE IF NOT EXISTS requests (id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER, method TEXT, path TEXT, status INTEGER, latency_ms INTEGER, target TEXT);",
            None,
        );
        let recent: Vec<RequestLogRow> = sql
            .exec(
                "SELECT ts,method,path,status,latency_ms,target FROM requests ORDER BY id DESC LIMIT 100;",
                None,
            )?
            .to_array()?;
        let connections = self.state.get_websockets().len();
        let last_seen = recent.first().map(|r| r.ts).unwrap_or(0);
        Response::from_json(&serde_json::json!({
            "connections": connections,
            "last_seen": last_seen,
            "recent": recent,
        }))
    }

    /// Append one request to the DO's own SQLite ring buffer (last ~500 rows).
    fn log_request(&self, method: &str, path: &str, status: u16, started_ms: i64, target: &str) {
        const RING_CAPACITY: i64 = 500;
        let latency = (Date::now().as_millis() as i64 - started_ms).max(0);
        let sql = self.state.storage().sql();
        let _ = sql.exec(
            "CREATE TABLE IF NOT EXISTS requests (id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER, method TEXT, path TEXT, status INTEGER, latency_ms INTEGER, target TEXT);",
            None,
        );
        let _ = sql.exec(
            "INSERT INTO requests (ts,method,path,status,latency_ms,target) VALUES (?,?,?,?,?,?);",
            vec![
                started_ms.into(),
                method.into(),
                path.into(),
                (status as i64).into(),
                latency.into(),
                target.into(),
            ],
        );
        let _ = sql.exec(
            "DELETE FROM requests WHERE id <= (SELECT MAX(id) FROM requests) - ?;",
            vec![RING_CAPACITY.into()],
        );
    }

    fn on_frame(&self, frame: Frame) {
        match frame {
            Frame::RespHead {
                stream,
                status,
                headers,
            } => {
                if let Some(p) = self.pending.borrow_mut().get_mut(&stream) {
                    if let Some(tx) = p.head.take() {
                        let _ = tx.send(Ok(RespHeadInfo { status, headers }));
                    }
                }
            }
            Frame::RespBody { stream, data } => {
                if let Some(p) = self.pending.borrow().get(&stream) {
                    let _ = p.body.unbounded_send(Ok(data));
                }
            }
            Frame::RespEnd { stream } => {
                self.pending.borrow_mut().remove(&stream);
                // Dropping Pending drops body_tx, ending the response stream.
            }
            Frame::StreamErr { stream, msg, .. } => {
                if let Some(mut p) = self.pending.borrow_mut().remove(&stream) {
                    if let Some(tx) = p.head.take() {
                        let _ = tx.send(Err(msg));
                    }
                } else if let Some(pub_ws) = self.ws_streams.borrow_mut().remove(&stream) {
                    let _ = pub_ws.close(Some(1011u16), Some("upstream error"));
                }
            }
            Frame::WsData {
                stream,
                binary,
                data,
            } => {
                // Clone the socket out under a short borrow; do not hold the
                // RefCell borrow across the (synchronous) send.
                let pub_ws = self.ws_streams.borrow().get(&stream).cloned();
                if let Some(pub_ws) = pub_ws {
                    if binary {
                        let _ = pub_ws.send_with_bytes(data);
                    } else {
                        let _ = pub_ws.send_with_str(String::from_utf8_lossy(&data));
                    }
                }
            }
            Frame::WsClose {
                stream,
                code,
                reason,
            } => {
                let pub_ws = self.ws_streams.borrow_mut().remove(&stream);
                if let Some(pub_ws) = pub_ws {
                    let _ = pub_ws.close(Some(code), Some(reason.as_str()));
                }
            }
            // The public socket is accepted the moment we create it, so the
            // client's WsAccept is informational for now.
            Frame::WsAccept { .. } => {}
            // Hello/HelloAck/etc. handled elsewhere.
            _ => {}
        }
    }
}
