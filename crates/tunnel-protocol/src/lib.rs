//! Shared wire format for the Tunnel service.
//!
//! Pure data + codec, no I/O. Compiles for native and `wasm32-unknown-unknown`.

mod codec;
mod frame;

use std::fmt;

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

/// Why a set of target names may not be advertised in a `Frame::Hello`.
#[derive(Debug, PartialEq, Eq)]
pub enum AdvertiseError {
    /// More names than [`MAX_ADVERTISED_TARGETS`] allows.
    TooMany { count: usize, cap: usize },
    /// A name longer than [`MAX_TARGET_NAME_BYTES`] allows.
    NameTooLong { name: String, cap: usize },
}

impl fmt::Display for AdvertiseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdvertiseError::TooMany { count, cap } => {
                write!(
                    f,
                    "{count} advertised targets exceeds the protocol cap of {cap}"
                )
            }
            AdvertiseError::NameTooLong { name, cap } => write!(
                f,
                "target name {name:?} is {} bytes, over the protocol cap of {cap}",
                name.len()
            ),
        }
    }
}

/// Checks that `targets` may be advertised in a `Frame::Hello`: at most
/// [`MAX_ADVERTISED_TARGETS`] names, each at most [`MAX_TARGET_NAME_BYTES`]
/// bytes long. Together the two caps keep the set inside the 2 KiB Durable
/// Object socket attachment limit.
///
/// An empty set is admissible: it advertises nothing and is capable of nothing.
/// Raises [`AdvertiseError::TooMany`] carrying the offending count, or
/// [`AdvertiseError::NameTooLong`] carrying the first offending name, each with
/// the cap it broke.
pub fn validate_advertised_targets(targets: &[String]) -> Result<(), AdvertiseError> {
    if targets.len() > MAX_ADVERTISED_TARGETS {
        return Err(AdvertiseError::TooMany {
            count: targets.len(),
            cap: MAX_ADVERTISED_TARGETS,
        });
    }
    if let Some(name) = targets.iter().find(|t| t.len() > MAX_TARGET_NAME_BYTES) {
        return Err(AdvertiseError::NameTooLong {
            name: name.clone(),
            cap: MAX_TARGET_NAME_BYTES,
        });
    }
    Ok(())
}

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

    fn names(count: usize) -> Vec<String> {
        (0..count).map(|i| format!("t{i}")).collect()
    }

    #[test]
    fn an_empty_advertised_set_is_admissible() {
        assert_eq!(validate_advertised_targets(&[]), Ok(()));
    }

    #[test]
    fn a_set_at_the_count_cap_is_admissible() {
        assert_eq!(
            validate_advertised_targets(&names(MAX_ADVERTISED_TARGETS)),
            Ok(())
        );
    }

    #[test]
    fn one_target_over_the_count_cap_carries_the_count() {
        assert_eq!(
            validate_advertised_targets(&names(MAX_ADVERTISED_TARGETS + 1)),
            Err(AdvertiseError::TooMany {
                count: MAX_ADVERTISED_TARGETS + 1,
                cap: MAX_ADVERTISED_TARGETS,
            })
        );
    }

    #[test]
    fn a_name_at_the_byte_cap_is_admissible() {
        let name = "t".repeat(MAX_TARGET_NAME_BYTES);
        assert_eq!(validate_advertised_targets(&[name]), Ok(()));
    }

    #[test]
    fn one_byte_over_the_name_cap_carries_the_name() {
        let name = "t".repeat(MAX_TARGET_NAME_BYTES + 1);
        assert_eq!(
            validate_advertised_targets(&["ok".to_string(), name.clone()]),
            Err(AdvertiseError::NameTooLong {
                name,
                cap: MAX_TARGET_NAME_BYTES,
            })
        );
    }

    #[test]
    fn the_name_cap_counts_bytes_not_characters() {
        let name = "é".repeat(MAX_TARGET_NAME_BYTES / 2 + 1);
        assert!(name.chars().count() <= MAX_TARGET_NAME_BYTES);
        assert!(matches!(
            validate_advertised_targets(&[name]),
            Err(AdvertiseError::NameTooLong { .. })
        ));
    }

    #[test]
    fn errors_name_the_offender_and_the_cap() {
        let too_many = AdvertiseError::TooMany {
            count: 33,
            cap: MAX_ADVERTISED_TARGETS,
        }
        .to_string();
        assert!(too_many.contains("33"), "{too_many}");
        assert!(too_many.contains("32"), "{too_many}");

        let too_long = AdvertiseError::NameTooLong {
            name: "gradio".to_string(),
            cap: MAX_TARGET_NAME_BYTES,
        }
        .to_string();
        assert!(too_long.contains("gradio"), "{too_long}");
        assert!(too_long.contains("6 bytes"), "{too_long}");
        assert!(too_long.contains("48"), "{too_long}");
    }
}
