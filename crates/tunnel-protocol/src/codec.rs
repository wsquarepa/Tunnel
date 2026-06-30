use crate::Frame;
use thiserror::Error;

/// Errors from (de)serializing a [`Frame`].
#[derive(Debug, Error)]
pub enum CodecError {
    #[error("postcard (de)serialization failed: {0}")]
    Postcard(#[from] postcard::Error),
}

/// Serialize a frame to its on-the-wire bytes (one binary WebSocket message).
pub fn encode(frame: &Frame) -> Result<Vec<u8>, CodecError> {
    Ok(postcard::to_allocvec(frame)?)
}

/// Deserialize a frame from a received binary WebSocket message.
pub fn decode(bytes: &[u8]) -> Result<Frame, CodecError> {
    Ok(postcard::from_bytes(bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StreamErrKind;

    fn round_trip(frame: Frame) {
        let bytes = encode(&frame).expect("encode");
        let decoded = decode(&bytes).expect("decode");
        assert_eq!(frame, decoded);
    }

    #[test]
    fn round_trips_hello() {
        round_trip(Frame::Hello {
            proto_version: 1,
            token: "tnl_abc".to_string(),
            agent_version: "0.1.0".to_string(),
            targets: vec!["jupyter".to_string(), "ollama".to_string()],
        });
    }

    #[test]
    fn round_trips_request_lifecycle() {
        round_trip(Frame::ReqHead {
            stream: 3,
            target: "jupyter".to_string(),
            method: "POST".to_string(),
            path: "/run".to_string(),
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            has_body: true,
        });
        round_trip(Frame::ReqBody { stream: 3, data: vec![1, 2, 3, 4] });
        round_trip(Frame::ReqEnd { stream: 3 });
    }

    #[test]
    fn round_trips_ws_and_control() {
        round_trip(Frame::WsData { stream: 9, binary: true, data: vec![0xff, 0x00] });
        round_trip(Frame::Credit { stream: 9, bytes: 65536 });
        round_trip(Frame::StreamErr {
            stream: 9,
            kind: StreamErrKind::DialFailed,
            msg: "connection refused".to_string(),
        });
    }

    #[test]
    fn decode_rejects_garbage() {
        // A truncated/invalid buffer must error, not panic.
        let err = decode(&[0xff, 0xff, 0xff, 0xff, 0xff]);
        assert!(err.is_err());
    }
}
