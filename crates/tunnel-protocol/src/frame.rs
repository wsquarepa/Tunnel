use serde::{Deserialize, Serialize};

/// Why a stream failed on the client side.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum StreamErrKind {
    /// The requested target name is not in the client's local allowlist.
    UnknownTarget,
    /// The client could not connect to the local target's port.
    DialFailed,
    /// The local target returned a transport-level error mid-stream.
    LocalError,
}

/// One multiplexed message on the control WebSocket.
///
/// Streams are always originated by the edge (Durable Object), which allocates
/// every `stream` id, so there is no id-collision/parity scheme.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    // ── handshake / control ──
    /// First frame from client → DO.
    Hello {
        proto_version: u16,
        token: String,
        agent_version: String,
        targets: Vec<String>,
    },
    /// DO → client acknowledgement of a valid `Hello`.
    HelloAck {
        session_id: u64,
        server_version: String,
    },
    /// Either direction: orderly teardown with a human-readable reason.
    Shutdown { reason: String },

    // ── HTTP request (DO → client) ──
    ReqHead {
        stream: u32,
        target: String,
        method: String,
        path: String,
        headers: Vec<(String, String)>,
        has_body: bool,
    },
    ReqBody {
        stream: u32,
        data: Vec<u8>,
    },
    ReqEnd {
        stream: u32,
    },

    // ── HTTP response (client → DO); SSE is RespBody flushed as it arrives ──
    RespHead {
        stream: u32,
        status: u16,
        headers: Vec<(String, String)>,
    },
    RespBody {
        stream: u32,
        data: Vec<u8>,
    },
    RespEnd {
        stream: u32,
    },

    // ── WebSocket passthrough (bidirectional) ──
    /// DO → client: an inbound Upgrade; client should dial the local WS.
    WsOpen {
        stream: u32,
        target: String,
        path: String,
        headers: Vec<(String, String)>,
    },
    /// client → DO: local WS accepted (status 101).
    WsAccept {
        stream: u32,
        status: u16,
        headers: Vec<(String, String)>,
    },
    /// Both directions: a WS message frame.
    WsData {
        stream: u32,
        binary: bool,
        data: Vec<u8>,
    },
    /// Both directions: WS close.
    WsClose {
        stream: u32,
        code: u16,
        reason: String,
    },

    // ── flow control, errors, cancellation ──
    /// Receiver grants the sender `bytes` more of in-flight body for `stream`.
    Credit {
        stream: u32,
        bytes: u32,
    },
    /// client → DO: this stream failed locally; DO maps it to a 502.
    StreamErr {
        stream: u32,
        kind: StreamErrKind,
        msg: String,
    },
    /// DO → client: the public peer disconnected; abandon this stream.
    Abort {
        stream: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_variants_construct_and_compare() {
        let head = Frame::ReqHead {
            stream: 7,
            target: "jupyter".to_string(),
            method: "GET".to_string(),
            path: "/api".to_string(),
            headers: vec![("accept".to_string(), "text/event-stream".to_string())],
            has_body: false,
        };
        assert_eq!(head.clone(), head);

        let err = Frame::StreamErr {
            stream: 7,
            kind: StreamErrKind::UnknownTarget,
            msg: "no such target".to_string(),
        };
        match err {
            Frame::StreamErr { kind, .. } => assert_eq!(kind, StreamErrKind::UnknownTarget),
            _ => panic!("wrong variant"),
        }
    }
}
