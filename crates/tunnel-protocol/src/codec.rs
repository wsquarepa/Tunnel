use crate::Frame;
use thiserror::Error;

/// Errors from (de)serializing a [`Frame`].
#[derive(Debug, Error)]
pub enum CodecError {
    #[error("postcard (de)serialization failed: {0}")]
    Postcard(#[from] postcard::Error),
    #[error("frame decoded but {0} trailing byte(s) remained")]
    TrailingBytes(usize),
}

/// Serialize a frame to its on-the-wire bytes (one binary WebSocket message).
pub fn encode(frame: &Frame) -> Result<Vec<u8>, CodecError> {
    Ok(postcard::to_allocvec(frame)?)
}

/// Deserialize a frame from a received binary WebSocket message.
pub fn decode(bytes: &[u8]) -> Result<Frame, CodecError> {
    let (frame, rest) = postcard::take_from_bytes::<Frame>(bytes)?;
    if !rest.is_empty() {
        return Err(CodecError::TrailingBytes(rest.len()));
    }
    Ok(frame)
}

/// Maximum body bytes carried in a single body frame.
///
/// Well under the 32 MiB WebSocket message ceiling so concurrent streams
/// interleave fairly instead of head-of-line blocking each other.
pub const MAX_BODY_CHUNK: usize = 1024 * 1024;

/// Bytes of body a sender may have in flight per stream before it must wait
/// for a `Frame::Credit`. This fixed window is used on both peers; dynamic /
/// auto-tuned windows are a future upgrade.
pub const INITIAL_CREDIT_WINDOW: u32 = 4 * MAX_BODY_CHUNK as u32;

/// Split a body into `<= MAX_BODY_CHUNK` slices for framing.
///
/// Empty input yields no chunks. Callers wrap each slice in the appropriate
/// body frame (`ReqBody` / `RespBody` / `WsData`).
pub fn body_chunks(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    data.chunks(MAX_BODY_CHUNK)
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
        round_trip(Frame::ReqBody {
            stream: 3,
            data: vec![1, 2, 3, 4],
        });
        round_trip(Frame::ReqEnd { stream: 3 });
    }

    #[test]
    fn round_trips_ws_and_control() {
        round_trip(Frame::WsData {
            stream: 9,
            binary: true,
            data: vec![0xff, 0x00],
        });
        round_trip(Frame::Credit {
            stream: 9,
            bytes: 65536,
        });
        round_trip(Frame::StreamErr {
            stream: 9,
            kind: StreamErrKind::DialFailed,
            msg: "connection refused".to_string(),
        });
    }

    #[test]
    fn decode_rejects_garbage() {
        // A malformed/incomplete varint must error, not panic.
        let err = decode(&[0xff, 0xff, 0xff, 0xff, 0xff]);
        assert!(err.is_err());
    }

    #[test]
    fn decode_rejects_trailing_bytes() {
        let mut bytes = encode(&Frame::ReqEnd { stream: 1 }).unwrap();
        bytes.push(0x00); // junk after a complete frame
        assert!(matches!(decode(&bytes), Err(CodecError::TrailingBytes(_))));
    }

    #[test]
    fn round_trips_every_variant() {
        let frames = vec![
            Frame::Hello {
                proto_version: 1,
                token: "tnl_abc".to_string(),
                agent_version: "0.1.0".to_string(),
                targets: vec!["jupyter".to_string()],
            },
            Frame::HelloAck {
                session_id: 42,
                server_version: "0.1.0".to_string(),
            },
            Frame::Shutdown {
                reason: "bye".to_string(),
            },
            Frame::ReqHead {
                stream: 1,
                target: "jupyter".to_string(),
                method: "GET".to_string(),
                path: "/api".to_string(),
                headers: vec![("accept".to_string(), "*/*".to_string())],
                has_body: false,
            },
            Frame::ReqBody {
                stream: 1,
                data: vec![1, 2, 3],
            },
            Frame::ReqEnd { stream: 1 },
            Frame::RespHead {
                stream: 1,
                status: 200,
                headers: vec![("content-type".to_string(), "text/plain".to_string())],
            },
            Frame::RespBody {
                stream: 1,
                data: vec![4, 5, 6],
            },
            Frame::RespEnd { stream: 1 },
            Frame::WsOpen {
                stream: 1,
                target: "socket".to_string(),
                path: "/ws".to_string(),
                headers: vec![("upgrade".to_string(), "websocket".to_string())],
            },
            Frame::WsAccept {
                stream: 1,
                status: 101,
                headers: vec![],
            },
            Frame::WsData {
                stream: 1,
                binary: true,
                data: vec![0xff, 0x00],
            },
            Frame::WsClose {
                stream: 1,
                code: 1000,
                reason: "normal".to_string(),
            },
            Frame::Credit {
                stream: 1,
                bytes: 65536,
            },
            Frame::StreamErr {
                stream: 1,
                kind: StreamErrKind::DialFailed,
                msg: "connection refused".to_string(),
            },
            Frame::Abort { stream: 1 },
        ];
        for frame in frames {
            assert_eq!(decode(&encode(&frame).unwrap()).unwrap(), frame);
        }
    }

    #[test]
    fn round_trips_max_body_chunk() {
        let frame = Frame::RespBody {
            stream: 1,
            data: vec![0xab; MAX_BODY_CHUNK],
        };
        assert_eq!(decode(&encode(&frame).unwrap()).unwrap(), frame);
    }

    #[test]
    fn empty_body_yields_no_chunks() {
        let chunks: Vec<&[u8]> = body_chunks(&[]).collect();
        assert!(chunks.is_empty());
    }

    #[test]
    fn small_body_is_one_chunk() {
        let data = vec![0u8; 10];
        let chunks: Vec<&[u8]> = body_chunks(&data).collect();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 10);
    }

    #[test]
    fn large_body_splits_on_cap_and_preserves_bytes() {
        let data = vec![7u8; MAX_BODY_CHUNK * 2 + 5];
        let chunks: Vec<&[u8]> = body_chunks(&data).collect();
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), MAX_BODY_CHUNK);
        assert_eq!(chunks[1].len(), MAX_BODY_CHUNK);
        assert_eq!(chunks[2].len(), 5);
        let reassembled: Vec<u8> = chunks.concat();
        assert_eq!(reassembled, data);
    }
}
