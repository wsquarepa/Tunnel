use anyhow::{anyhow, Result};
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tunnel_protocol::{decode, encode, Frame, StreamErrKind, PROTO_VERSION};

use crate::config::Config;

pub type Outbound = mpsc::UnboundedSender<Frame>;

/// Map of active stream id → sender feeding that stream's task.
pub type Streams = Arc<Mutex<HashMap<u32, mpsc::UnboundedSender<Frame>>>>;

pub async fn run(cfg: Config, token: String) -> Result<()> {
    let connect_url = format!("{}/_tunnel/connect", cfg.worker_url.trim_end_matches('/'));
    let mut request = connect_url.into_client_request()?;
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse()?);

    let (ws, _resp) = tokio_tungstenite::connect_async(request).await?;
    let (mut sink, mut stream) = ws.split();

    // Writer task: owns the sink, drains the outbound channel.
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Frame>();
    let writer = tokio::spawn(async move {
        while let Some(frame) = out_rx.recv().await {
            let bytes = match encode(&frame) {
                Ok(b) => b,
                Err(e) => {
                    tracing::error!("encode failed: {e}");
                    continue;
                }
            };
            if sink.send(Message::Binary(bytes)).await.is_err() {
                break;
            }
        }
    });

    // Handshake.
    let targets: Vec<String> = cfg.targets.keys().cloned().collect();
    out_tx.send(Frame::Hello {
        proto_version: PROTO_VERSION,
        token: token.clone(),
        agent_version: env!("CARGO_PKG_VERSION").to_string(),
        targets,
    })?;

    let streams: Streams = Arc::new(Mutex::new(HashMap::new()));
    let cfg = Arc::new(cfg);

    while let Some(msg) = stream.next().await {
        let msg = msg?;
        let bytes = match msg {
            Message::Binary(b) => b,
            Message::Close(_) => break,
            Message::Ping(_) | Message::Pong(_) | Message::Text(_) | Message::Frame(_) => continue,
        };
        let frame = decode(&bytes).map_err(|e| anyhow!("decode: {e}"))?;
        dispatch(frame, &cfg, &out_tx, &streams).await;
    }

    drop(out_tx);
    let _ = writer.await;
    Ok(())
}

async fn dispatch(frame: Frame, cfg: &Arc<Config>, out: &Outbound, streams: &Streams) {
    match frame {
        Frame::HelloAck { server_version, .. } => {
            tracing::info!("connected; server {server_version}");
        }
        Frame::Shutdown { reason } => {
            tracing::warn!("server shutdown: {reason}");
        }
        Frame::ReqHead {
            stream,
            target,
            method,
            path,
            headers,
            ..
        } => match cfg.target_addr(&target) {
            Some(addr) => {
                let (tx, rx) = mpsc::unbounded_channel::<Frame>();
                streams.lock().await.insert(stream, tx);
                let addr = addr.to_string();
                tokio::spawn(crate::http_proxy::handle(
                    stream,
                    method,
                    path,
                    headers,
                    rx,
                    addr,
                    out.clone(),
                ));
            }
            None => {
                let _ = out.send(Frame::StreamErr {
                    stream,
                    kind: StreamErrKind::UnknownTarget,
                    msg: format!("unknown target: {target}"),
                });
            }
        },
        f @ (Frame::ReqBody { .. } | Frame::ReqEnd { .. } | Frame::Abort { .. }) => {
            let stream = match &f {
                Frame::ReqBody { stream, .. }
                | Frame::ReqEnd { stream }
                | Frame::Abort { stream } => *stream,
                _ => unreachable!(),
            };
            if let Some(tx) = streams.lock().await.get(&stream) {
                let _ = tx.send(f);
            }
        }
        // WsOpen spawns a stream task; WsData / WsClose route to an existing stream (Task 5).
        other => {
            let _ = (cfg, out, streams, other);
        }
    }
}
