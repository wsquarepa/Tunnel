use futures::{SinkExt, StreamExt};
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tunnel_protocol::{Frame, StreamErrKind};

use crate::conn::Outbound;

pub async fn handle(
    stream: u32,
    path: String,
    addr: String,
    mut frame_rx: mpsc::UnboundedReceiver<Frame>,
    out: Outbound,
) {
    let started = Instant::now();
    tracing::debug!(path = %path, addr = %addr, "ws stream open");

    if !path.starts_with('/') {
        tracing::debug!(path = %path, "invalid path");
        let _ = out.send(Frame::StreamErr {
            stream,
            kind: StreamErrKind::LocalError,
            msg: "invalid path".into(),
        });
        return;
    }

    let url = format!("ws://{addr}{path}");
    let local = match tokio_tungstenite::connect_async(&url).await {
        Ok((ws, _)) => ws,
        Err(e) => {
            tracing::debug!(addr = %addr, error = %e, "local ws dial failed");
            let _ = out.send(Frame::StreamErr {
                stream,
                kind: StreamErrKind::DialFailed,
                msg: e.to_string(),
            });
            return;
        }
    };
    let (mut local_sink, mut local_stream) = local.split();
    let _ = out.send(Frame::WsAccept {
        stream,
        status: 101,
        headers: vec![],
    });
    tracing::debug!(
        elapsed_ms = started.elapsed().as_millis() as u64,
        "ws accepted"
    );

    loop {
        tokio::select! {
            // tunnel → local
            incoming = frame_rx.recv() => match incoming {
                Some(Frame::WsData { binary, data, .. }) => {
                    tracing::trace!(binary, len = data.len(), "ws data edge->local");
                    let msg = if binary {
                        Message::Binary(data)
                    } else {
                        Message::Text(String::from_utf8_lossy(&data).into_owned())
                    };
                    if local_sink.send(msg).await.is_err() {
                        break;
                    }
                }
                Some(Frame::WsClose { .. }) | Some(Frame::Abort { .. }) | None => {
                    tracing::debug!(elapsed_ms = started.elapsed().as_millis() as u64, "ws closed by edge");
                    let _ = local_sink.send(Message::Close(None)).await;
                    break;
                }
                Some(_) => {}
            },
            // local → tunnel
            outgoing = local_stream.next() => match outgoing {
                Some(Ok(Message::Binary(data))) => {
                    tracing::trace!(len = data.len(), "ws data local->edge");
                    let _ = out.send(Frame::WsData { stream, binary: true, data });
                }
                Some(Ok(Message::Text(text))) => {
                    tracing::trace!(len = text.len(), "ws data local->edge");
                    let _ = out.send(Frame::WsData { stream, binary: false, data: text.into_bytes() });
                }
                Some(Ok(Message::Close(_))) | None => {
                    tracing::debug!(elapsed_ms = started.elapsed().as_millis() as u64, "ws closed by local");
                    let _ = out.send(Frame::WsClose { stream, code: 1000, reason: String::new() });
                    break;
                }
                // Ping/Pong/raw frames are handled by the library; nothing to forward.
                Some(Ok(_)) => {}
                Some(Err(e)) => {
                    tracing::debug!(error = %e, "ws local error");
                    let _ = out.send(Frame::WsClose { stream, code: 1011, reason: e.to_string() });
                    break;
                }
            },
        }
    }
}
