use anyhow::{anyhow, Result};
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{interval_at, Instant, MissedTickBehavior};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tracing::Instrument;
use tunnel_protocol::{decode, encode, Frame, StreamErrKind, PROTO_VERSION};

use crate::config::Config;
use crate::dial;
use crate::liveness::{LinkState, LivenessTracker};

/// Ping cadence for an otherwise idle control socket. NAT and stateful
/// firewalls on the path evict idle flows (commonly after 15-60 minutes) and
/// then reset the connection; periodic protocol-level pings keep the flow
/// entry alive. Cloudflare's runtime answers these pings at the edge without
/// waking a hibernated Durable Object.
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);

/// The DO acknowledges a valid Hello immediately; a missing HelloAck means
/// the control channel is not actually functional even though the socket
/// opened, so treat it as a failed connect rather than sitting half-open.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// How often the reader loop re-evaluates link liveness. Finer than
/// `KEEPALIVE_INTERVAL` on purpose: the check must run even while the writer
/// is parked inside a `send` on a socket whose send buffer has filled, which
/// is exactly the case a keepalive-driven check cannot cover.
const LIVENESS_CHECK_INTERVAL: Duration = Duration::from_secs(10);

pub type Outbound = mpsc::UnboundedSender<Frame>;

/// Map of active stream id → sender feeding that stream's task.
pub type Streams = Arc<Mutex<HashMap<u32, mpsc::UnboundedSender<Frame>>>>;

/// Wire-variant name for trace logging; the Frame enum has no Display and
/// Debug would drag payload bytes into the log.
fn frame_name(f: &Frame) -> &'static str {
    match f {
        Frame::Hello { .. } => "Hello",
        Frame::HelloAck { .. } => "HelloAck",
        Frame::Shutdown { .. } => "Shutdown",
        Frame::ReqHead { .. } => "ReqHead",
        Frame::ReqBody { .. } => "ReqBody",
        Frame::ReqEnd { .. } => "ReqEnd",
        Frame::RespHead { .. } => "RespHead",
        Frame::RespBody { .. } => "RespBody",
        Frame::RespEnd { .. } => "RespEnd",
        Frame::WsOpen { .. } => "WsOpen",
        Frame::WsAccept { .. } => "WsAccept",
        Frame::WsData { .. } => "WsData",
        Frame::WsClose { .. } => "WsClose",
        Frame::Credit { .. } => "Credit",
        Frame::StreamErr { .. } => "StreamErr",
        Frame::Abort { .. } => "Abort",
    }
}

fn frame_stream(f: &Frame) -> Option<u32> {
    match f {
        Frame::Hello { .. } | Frame::HelloAck { .. } | Frame::Shutdown { .. } => None,
        Frame::ReqHead { stream, .. }
        | Frame::ReqBody { stream, .. }
        | Frame::ReqEnd { stream }
        | Frame::RespHead { stream, .. }
        | Frame::RespBody { stream, .. }
        | Frame::RespEnd { stream }
        | Frame::WsOpen { stream, .. }
        | Frame::WsAccept { stream, .. }
        | Frame::WsData { stream, .. }
        | Frame::WsClose { stream, .. }
        | Frame::Credit { stream, .. }
        | Frame::StreamErr { stream, .. }
        | Frame::Abort { stream } => Some(*stream),
    }
}

/// Why the writer task exited; `run` turns each cause into the right
/// teardown behavior and log line.
enum WriterExit {
    ChannelClosed,
    SendFailed,
}

pub async fn run(
    cfg: Config,
    token: String,
    acked_at: Arc<OnceLock<std::time::Instant>>,
) -> Result<()> {
    let started = std::time::Instant::now();
    let connect_url = format!("{}/_tunnel/connect", cfg.worker_url.trim_end_matches('/'));
    let mut request = connect_url.into_client_request()?;
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse()?);

    let ws = dial::connect(request).await?;
    let (mut sink, mut stream) = ws.split();

    let tracker = Arc::new(Mutex::new(LivenessTracker::new(std::time::Instant::now())));

    // Writer task: owns the sink, drains the outbound channel, and pings on
    // idle to keep the path's NAT/firewall state alive. It never judges
    // liveness: a `send` here can park indefinitely on a dead socket, so the
    // verdict belongs to the reader loop, which aborts this task.
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Frame>();
    let writer_tracker = tracker.clone();
    let writer = tokio::spawn(
        async move {
            let mut keepalive =
                interval_at(Instant::now() + KEEPALIVE_INTERVAL, KEEPALIVE_INTERVAL);
            keepalive.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    frame = out_rx.recv() => {
                        let Some(frame) = frame else { return WriterExit::ChannelClosed };
                        let bytes = match encode(&frame) {
                            Ok(b) => b,
                            Err(e) => {
                                tracing::error!(error = %e, "encode failed");
                                continue;
                            }
                        };
                        tracing::trace!(
                            frame = frame_name(&frame),
                            stream = frame_stream(&frame),
                            len = bytes.len(),
                            "send"
                        );
                        if sink.send(Message::Binary(bytes)).await.is_err() {
                            return WriterExit::SendFailed;
                        }
                    }
                    _ = keepalive.tick() => {
                        // Arm the liveness deadline before the send, not after:
                        // a send that never returns is itself the symptom the
                        // reader's check has to be able to see.
                        writer_tracker.lock().await.on_ping_sent();
                        if sink.send(Message::Ping(Vec::new())).await.is_err() {
                            return WriterExit::SendFailed;
                        }
                        tracing::debug!("ping sent");
                    }
                }
            }
        }
        .in_current_span(),
    );
    tokio::pin!(writer);

    // Handshake.
    let targets: Vec<String> = cfg.targets.keys().cloned().collect();
    out_tx.send(Frame::Hello {
        proto_version: PROTO_VERSION,
        token: token.clone(),
        agent_version: env!("CARGO_PKG_VERSION").to_string(),
        targets: targets.clone(),
    })?;
    tracing::debug!(proto = PROTO_VERSION, targets = targets.len(), "hello sent");

    let streams: Streams = Arc::new(Mutex::new(HashMap::new()));
    let cfg = Arc::new(cfg);

    // The ack deadline is armed until the HelloAck arrives; the DO may
    // legitimately dispatch requests to a pooled socket before the handshake
    // completes, so non-ack frames are processed normally while waiting.
    let mut acked = false;
    let ack_deadline = tokio::time::sleep(HANDSHAKE_TIMEOUT);
    tokio::pin!(ack_deadline);

    let mut liveness_check = interval_at(
        Instant::now() + LIVENESS_CHECK_INTERVAL,
        LIVENESS_CHECK_INTERVAL,
    );
    liveness_check.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let result: Result<()> = loop {
        tokio::select! {
            _ = &mut ack_deadline, if !acked => {
                break Err(anyhow!(
                    "no HelloAck within {HANDSHAKE_TIMEOUT:?}; control channel not functional"
                ));
            }
            _ = liveness_check.tick() => {
                let now = std::time::Instant::now();
                let (state, silence) = {
                    let t = tracker.lock().await;
                    (t.state(now), t.silence(now))
                };
                if state == LinkState::Dead {
                    tracing::warn!(
                        silence_s = silence.as_secs(),
                        "link dead: no inbound traffic despite keepalives"
                    );
                    // Aborting drops the sink, which closes the socket and
                    // releases a writer parked in a send that will never drain.
                    writer.abort();
                    break Err(anyhow!(
                        "link presumed dead: no inbound traffic for {:?}",
                        crate::liveness::DEAD_AFTER
                    ));
                }
            }
            exit = &mut writer => {
                break match exit {
                    Ok(WriterExit::SendFailed) => Err(anyhow!("control socket send failed")),
                    Ok(WriterExit::ChannelClosed) | Err(_) => Ok(()),
                };
            }
            msg = stream.next() => {
                let Some(msg) = msg else { break Ok(()) };
                let msg = match msg {
                    Ok(m) => m,
                    Err(e) => break Err(e.into()),
                };
                tracker.lock().await.on_traffic(std::time::Instant::now());
                let bytes = match msg {
                    Message::Binary(b) => b,
                    Message::Close(frame) => {
                        tracing::debug!(frame = ?frame, "close frame received");
                        break Ok(());
                    }
                    Message::Pong(_) => {
                        tracing::debug!("pong received");
                        continue;
                    }
                    Message::Ping(_) | Message::Text(_) | Message::Frame(_) => continue,
                };
                let frame = match decode(&bytes) {
                    Ok(f) => f,
                    Err(e) => break Err(anyhow!("decode: {e}")),
                };
                tracing::trace!(
                    frame = frame_name(&frame),
                    stream = frame_stream(&frame),
                    len = bytes.len(),
                    "recv"
                );
                match &frame {
                    Frame::Shutdown { reason } => {
                        tracing::warn!(reason = %reason, "server shutdown");
                        break Ok(());
                    }
                    Frame::HelloAck { session_id, server_version } => {
                        acked = true;
                        // Uptime for the caller's backoff is measured from
                        // here, so a socket that opened but never acked scores
                        // zero and escalates the retry delay.
                        let _ = acked_at.set(std::time::Instant::now());
                        tracing::Span::current().record("session_id", *session_id);
                        tracing::info!(
                            session = *session_id,
                            server = %server_version,
                            elapsed_ms = started.elapsed().as_millis() as u64,
                            "connected"
                        );
                    }
                    _ => dispatch(frame, &cfg, &out_tx, &streams).await,
                }
            }
        }
    };

    drop(out_tx);
    // The writer future may already have completed via the select arm; a
    // second await on the pinned JoinHandle would panic, so only await it
    // when the loop ended for another reason.
    if !writer.is_finished() {
        let _ = writer.as_mut().await;
    }
    result
}

async fn dispatch(frame: Frame, cfg: &Arc<Config>, out: &Outbound, streams: &Streams) {
    match frame {
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
                let out = out.clone();
                let streams_cleanup = streams.clone();
                tracing::trace!(stream, headers = ?headers, "request headers");
                let span = tracing::info_span!("stream", id = stream, target = %target);
                tokio::spawn(
                    async move {
                        crate::http_proxy::handle(stream, method, path, headers, rx, addr, out)
                            .await;
                        streams_cleanup.lock().await.remove(&stream);
                    }
                    .instrument(span),
                );
            }
            None => {
                tracing::warn!(stream, target = %target, "unknown target");
                let _ = out.send(Frame::StreamErr {
                    stream,
                    kind: StreamErrKind::UnknownTarget,
                    msg: format!("unknown target: {target}"),
                });
            }
        },
        Frame::WsOpen {
            stream,
            target,
            path,
            headers,
        } => match cfg.target_addr(&target) {
            Some(addr) => {
                let (tx, rx) = mpsc::unbounded_channel::<Frame>();
                streams.lock().await.insert(stream, tx);
                let addr = addr.to_string();
                let out = out.clone();
                let streams_cleanup = streams.clone();
                tracing::trace!(stream, headers = ?headers, "request headers");
                let span = tracing::info_span!("stream", id = stream, target = %target);
                tokio::spawn(
                    async move {
                        crate::ws_proxy::handle(stream, path, addr, rx, out).await;
                        streams_cleanup.lock().await.remove(&stream);
                    }
                    .instrument(span),
                );
            }
            None => {
                tracing::warn!(stream, target = %target, "unknown target");
                let _ = out.send(Frame::StreamErr {
                    stream,
                    kind: StreamErrKind::UnknownTarget,
                    msg: format!("unknown target: {target}"),
                });
            }
        },
        f @ (Frame::ReqBody { .. }
        | Frame::ReqEnd { .. }
        | Frame::Abort { .. }
        | Frame::WsData { .. }
        | Frame::WsClose { .. }) => {
            let stream = match &f {
                Frame::ReqBody { stream, .. }
                | Frame::ReqEnd { stream }
                | Frame::Abort { stream }
                | Frame::WsData { stream, .. }
                | Frame::WsClose { stream, .. } => *stream,
                _ => unreachable!(),
            };
            if let Some(tx) = streams.lock().await.get(&stream) {
                let _ = tx.send(f);
            }
        }
        other => {
            let _ = (cfg, out, streams, other);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_name_covers_representative_variants() {
        assert_eq!(
            frame_name(&Frame::Hello {
                proto_version: 1,
                token: String::new(),
                agent_version: String::new(),
                targets: vec![]
            }),
            "Hello"
        );
        assert_eq!(
            frame_name(&Frame::RespBody {
                stream: 1,
                data: vec![]
            }),
            "RespBody"
        );
        assert_eq!(frame_name(&Frame::Abort { stream: 1 }), "Abort");
    }

    #[test]
    fn frame_stream_extracts_id_where_present() {
        assert_eq!(frame_stream(&Frame::ReqEnd { stream: 42 }), Some(42));
        assert_eq!(
            frame_stream(&Frame::Shutdown {
                reason: String::new()
            }),
            None
        );
    }
}
