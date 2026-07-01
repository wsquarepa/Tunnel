use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::sync::OnceLock;
use tokio::sync::mpsc;
use tunnel_protocol::{body_chunks, Frame, StreamErrKind};

use crate::conn::Outbound;

fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

pub async fn handle(
    stream: u32,
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    mut body_rx: mpsc::UnboundedReceiver<Frame>,
    addr: String,
    out: Outbound,
) {
    // Collect the request body (buffered for now; streaming uploads are a future upgrade).
    let mut body: Vec<u8> = Vec::new();
    while let Some(frame) = body_rx.recv().await {
        match frame {
            Frame::ReqBody { data, .. } => body.extend_from_slice(&data),
            Frame::ReqEnd { .. } => break,
            Frame::Abort { .. } => return,
            _ => {}
        }
    }

    if !path.starts_with('/') {
        let _ = out.send(Frame::StreamErr {
            stream,
            kind: StreamErrKind::LocalError,
            msg: "invalid path".into(),
        });
        return;
    }

    let url = format!("http://{addr}{path}");
    let mut header_map = HeaderMap::new();
    for (k, v) in &headers {
        if let (Ok(name), Ok(val)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(v),
        ) {
            header_map.insert(name, val);
        }
    }
    let method = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);

    let resp = match http_client()
        .request(method, &url)
        .headers(header_map)
        .body(body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = out.send(Frame::StreamErr {
                stream,
                kind: StreamErrKind::DialFailed,
                msg: e.to_string(),
            });
            return;
        }
    };

    let status = resp.status().as_u16();
    let resp_headers: Vec<(String, String)> = resp
        .headers()
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|s| (k.to_string(), s.to_string())))
        .collect();
    let _ = out.send(Frame::RespHead {
        stream,
        status,
        headers: resp_headers,
    });

    // Stream the response body so SSE / chunked responses flush incrementally.
    let mut byte_stream = resp.bytes_stream();
    while let Some(chunk) = byte_stream.next().await {
        match chunk {
            Ok(bytes) => {
                for piece in body_chunks(&bytes) {
                    if out
                        .send(Frame::RespBody {
                            stream,
                            data: piece.to_vec(),
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }
            Err(e) => {
                let _ = out.send(Frame::StreamErr {
                    stream,
                    kind: StreamErrKind::LocalError,
                    msg: e.to_string(),
                });
                return;
            }
        }
    }
    let _ = out.send(Frame::RespEnd { stream });
}
