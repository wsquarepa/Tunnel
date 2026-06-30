//! Shared wire format for the Tunnel service.
//!
//! Pure data + codec, no I/O. Compiles for native and `wasm32-unknown-unknown`.

mod frame;

pub use frame::{Frame, StreamErrKind};

/// Current protocol version, sent in the handshake `Frame::Hello`.
pub const PROTO_VERSION: u16 = 1;

/// Returns whether a peer's advertised protocol version is compatible with ours.
///
/// This requires an exact match; a future version may widen this to a range.
pub fn is_compatible(peer: u16) -> bool {
    peer == PROTO_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proto_version_is_one() {
        assert_eq!(PROTO_VERSION, 1);
    }

    #[test]
    fn same_version_is_compatible() {
        assert!(is_compatible(PROTO_VERSION));
    }

    #[test]
    fn different_version_is_incompatible() {
        assert!(!is_compatible(PROTO_VERSION + 1));
    }
}
