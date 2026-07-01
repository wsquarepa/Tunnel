use futures::{SinkExt, StreamExt};
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
    if !path.starts_with('/') {
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

    loop {
        tokio::select! {
            // tunnel → local
            incoming = frame_rx.recv() => match incoming {
                Some(Frame::WsData { binary, data, .. }) => {
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
                    let _ = local_sink.send(Message::Close(None)).await;
                    break;
                }
                Some(_) => {}
            },
            // local → tunnel
            outgoing = local_stream.next() => match outgoing {
                Some(Ok(Message::Binary(data))) => {
                    let _ = out.send(Frame::WsData { stream, binary: true, data });
                }
                Some(Ok(Message::Text(text))) => {
                    let _ = out.send(Frame::WsData { stream, binary: false, data: text.into_bytes() });
                }
                Some(Ok(Message::Close(_))) | None => {
                    let _ = out.send(Frame::WsClose { stream, code: 1000, reason: String::new() });
                    break;
                }
                // Ping/Pong/raw frames are handled by the library; nothing to forward.
                Some(Ok(_)) => {}
                Some(Err(e)) => {
                    let _ = out.send(Frame::WsClose { stream, code: 1011, reason: e.to_string() });
                    break;
                }
            },
        }
    }
}
