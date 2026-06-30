use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use futures::channel::{mpsc, oneshot};
use futures::StreamExt;
use tunnel_protocol::{decode, encode, Frame};
use worker::*;

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
    env: Env,
    next_stream: Rc<RefCell<u32>>,
    pending: Rc<RefCell<HashMap<u32, Pending>>>,
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
    fn new(state: State, env: Env) -> Self {
        Self {
            state,
            env,
            next_stream: Rc::new(RefCell::new(0)),
            pending: Rc::new(RefCell::new(HashMap::new())),
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
            self.handle_req(req).await
        } else {
            Response::error("not found", 404)
        }
    }

    async fn websocket_message(
        &self,
        _ws: WebSocket,
        message: WebSocketIncomingMessage,
    ) -> Result<()> {
        let bytes = match message {
            WebSocketIncomingMessage::Binary(b) => b,
            // Control frames never arrive as text in this protocol; ignore.
            WebSocketIncomingMessage::String(_) => return Ok(()),
        };
        let frame = decode(&bytes).map_err(|e| Error::RustError(e.to_string()))?;
        self.on_frame(frame);
        Ok(())
    }

    async fn websocket_close(
        &self,
        _ws: WebSocket,
        _code: usize,
        _reason: String,
        _clean: bool,
    ) -> Result<()> {
        // Pool membership is read live from get_websockets(); nothing to prune here.
        Ok(())
    }
}

impl TunnelSession {
    async fn handle_connect(&self) -> Result<Response> {
        let pair = WebSocketPair::new()?;
        self.state.accept_web_socket(&pair.server);
        Response::from_websocket(pair.client)
    }

    async fn handle_req(&self, req: Request) -> Result<Response> {
        let Some(ws) = self.pick_socket() else {
            return Response::error("tunnel offline", 502);
        };
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

        // Spike: forward method/path only, no request body.
        let url = req.url()?;
        Self::send_frame(
            &ws,
            &Frame::ReqHead {
                stream,
                target: "spike".to_string(),
                method: req.method().to_string(),
                path: url.path().to_string(),
                headers: vec![],
                has_body: false,
            },
        )?;
        Self::send_frame(&ws, &Frame::ReqEnd { stream })?;

        match head_rx.await {
            Ok(Ok(head)) => {
                let headers = Headers::new();
                for (k, v) in &head.headers {
                    headers.set(k, v)?;
                }
                let body = body_rx.map(|chunk| chunk.map_err(Error::RustError));
                Ok(Response::from_stream(body)?
                    .with_status(head.status)
                    .with_headers(headers))
            }
            Ok(Err(msg)) => Response::error(format!("upstream error: {msg}"), 502),
            Err(_) => Response::error("tunnel closed", 502),
        }
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
                }
            }
            // Hello/HelloAck/Ws*/etc. handled in later tasks.
            _ => {}
        }
    }
}
