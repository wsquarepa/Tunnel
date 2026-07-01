//! Test origin exposing: POST /echo (body echo), GET /sse (3 events), GET /ws (echo).
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::sse::{Event, Sse};
use axum::routing::{get, post};
use axum::Router;
use futures::stream::{self, Stream, StreamExt};
use std::convert::Infallible;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/echo", post(|body: String| async move { body }))
        .route("/sse", get(sse))
        .route(
            "/ws",
            get(|up: WebSocketUpgrade| async move { up.on_upgrade(ws_echo) }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:9099")
        .await
        .unwrap();
    println!("dummy origin on 127.0.0.1:9099");
    axum::serve(listener, app).await.unwrap();
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
