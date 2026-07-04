//! Test origin exposing: POST /echo (body echo), GET /sse (3 events), GET /ws
//! (echo), GET /headers (request headers as JSON), GET /hang (never responds),
//! GET /whoami (this origin's identity), GET /slow (responds after 6s, naming
//! the identity). `DUMMY_ORIGIN_PORT` and `DUMMY_ORIGIN_ID` select the port and
//! identity so the e2e pool stage can run two distinguishable origins at once.
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::sse::{Event, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream::{self, Stream, StreamExt};
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::time::Duration;

/// Long enough for the e2e pool stage to overlap two requests (1s apart) and
/// to kill a client while its request is still in flight.
const SLOW_RESPONSE_DELAY: Duration = Duration::from_secs(6);

#[tokio::main]
async fn main() {
    let port: u16 = match std::env::var("DUMMY_ORIGIN_PORT") {
        Ok(v) => v.parse().expect("DUMMY_ORIGIN_PORT must be a port number"),
        Err(_) => 9099,
    };
    let ident = std::env::var("DUMMY_ORIGIN_ID").unwrap_or_else(|_| "origin".to_string());
    let whoami_ident = ident.clone();
    let slow_ident = ident.clone();
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
        )
        // Identifies which pool member served a request.
        .route(
            "/whoami",
            get(move || {
                let id = whoami_ident.clone();
                async move { id }
            }),
        )
        // Long-poll used by the e2e pool stage; see SLOW_RESPONSE_DELAY.
        .route(
            "/slow",
            get(move || {
                let id = slow_ident.clone();
                async move {
                    tokio::time::sleep(SLOW_RESPONSE_DELAY).await;
                    format!("slow:{id}")
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .unwrap();
    println!("dummy origin on 127.0.0.1:{port}");
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
