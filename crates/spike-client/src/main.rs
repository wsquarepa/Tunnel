//! Spike-only native client: connect to a locally-running worker, answer one
//! request with a fixed response. It exists because the worker speaks the
//! postcard `Frame` protocol, which a Node client cannot. The real
//! `tunnel-client` (Plan 3) replaces it.
//!
//! Run with:
//!   cargo run -p spike-client -- ws://127.0.0.1:8787/_tunnel/connect
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tunnel_protocol::{decode, encode, Frame, PROTO_VERSION};

#[tokio::main]
async fn main() {
    let url = std::env::args().nth(1).expect("usage: spike-client <ws-url>");
    let (mut ws, _) = connect_async(url).await.expect("connect");
    eprintln!("spike-client: connected, sending Hello");
    let hello = Frame::Hello {
        proto_version: PROTO_VERSION,
        token: String::new(),
        agent_version: env!("CARGO_PKG_VERSION").to_string(),
        targets: vec!["spike".to_string()],
    };
    ws.send(Message::Binary(encode(&hello).unwrap().into()))
        .await
        .unwrap();
    eprintln!("spike-client: waiting for HelloAck / ReqHead");
    while let Some(msg) = ws.next().await {
        let Ok(Message::Binary(bytes)) = msg else {
            continue;
        };
        let frame = decode(&bytes).expect("decode");
        eprintln!("spike-client: received {frame:?}");
        if let Frame::HelloAck { .. } = frame {
            eprintln!("spike-client: handshake complete, awaiting requests");
            continue;
        }
        if let Frame::ReqHead { stream, .. } = frame {
            for f in [
                Frame::RespHead {
                    stream,
                    status: 200,
                    headers: vec![("content-type".into(), "text/plain".into())],
                },
                Frame::RespBody {
                    stream,
                    data: b"hello from spike".to_vec(),
                },
                Frame::RespEnd { stream },
            ] {
                ws.send(Message::Binary(encode(&f).unwrap().into()))
                    .await
                    .unwrap();
            }
            eprintln!("spike-client: answered stream {stream}");
        }
    }
}
