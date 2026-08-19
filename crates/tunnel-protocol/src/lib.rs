//! Shared wire format for the Tunnel service.
//!
//! Pure data + codec, no I/O. Compiles for native and `wasm32-unknown-unknown`.

mod codec;
mod frame;

pub use frame::{Frame, StreamErrKind};

pub use codec::{body_chunks, decode, encode, CodecError, INITIAL_CREDIT_WINDOW, MAX_BODY_CHUNK};

/// Current protocol version, sent in the handshake `Frame::Hello`.
pub const PROTO_VERSION: u16 = 1;

/// Most target names one `Frame::Hello` may advertise. Together with
/// [`MAX_TARGET_NAME_BYTES`] this bounds the advertised set so it fits the
/// 2 KiB Durable Object socket attachment limit.
pub const MAX_ADVERTISED_TARGETS: usize = 32;

/// Longest target name, in bytes, one `Frame::Hello` may advertise. Together
/// with [`MAX_ADVERTISED_TARGETS`] this bounds the advertised set so it fits
/// the 2 KiB Durable Object socket attachment limit.
pub const MAX_TARGET_NAME_BYTES: usize = 48;

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
