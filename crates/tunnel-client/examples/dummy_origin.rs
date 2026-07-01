//! Test origin exposing: POST /echo (body echo), GET /sse (3 events), GET /ws
//! (echo), GET /headers (request headers as JSON), GET /hang (never responds).
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::sse::{Event, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream::{self, Stream, StreamExt};
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/echo", post(|body: String| async move { body }))
        .route("/sse", get(sse))
        .route(
            "/ws",
            get(|up: WebSocketUpgrade| async move { up.on_upgrade(ws_echo) }),
        )
        // Reflects the headers the origin actually received, so tests can confirm
        // custom headers are forwarded and internal x-tunnel-* are stripped.
        .route("/headers", get(headers))
        // Never responds: exercises the edge head-timeout (504) backstop.
        .route(
            "/hang",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(3600)).await;
                "unreachable"
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:9099")
        .await
        .unwrap();
    println!("dummy origin on 127.0.0.1:9099");
    axum::serve(listener, app).await.unwrap();
}

async fn headers(headers: HeaderMap) -> Json<BTreeMap<String, String>> {
    let map = headers
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    Json(map)
}

async fn sse() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let s = stream::iter(0..3).map(|i| Ok(Event::default().data(format!("event-{i}"))));
    Sse::new(s)
}

async fn ws_echo(mut socket: WebSocket) {
    while let Some(Ok(msg)) = socket.recv().await {
        if let Message::Text(t) = msg {
            if socket
                .send(Message::Text(format!("echo:{t}")))
                .await
                .is_err()
            {
                break;
            }
        }
    }
}
