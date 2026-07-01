use sha2::{Digest, Sha256};

const BASE62: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

pub fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

/// Returns (token, sha256_hex(token), display_prefix).
pub fn generate() -> (String, String, String) {
    let mut raw = [0u8; 32];
    getrandom::getrandom(&mut raw).expect("getrandom");
    let body: String = raw
        .iter()
        .map(|b| BASE62[(*b as usize) % 62] as char)
        .collect();
    let token = format!("tnl_{body}");
    let hash = sha256_hex(&token);
    let prefix = token.chars().take(12).collect::<String>();
    (token, hash, prefix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_is_stable() {
        assert_eq!(sha256_hex("tnl_abc").len(), 64);
        assert_eq!(sha256_hex("tnl_abc"), sha256_hex("tnl_abc"));
    }

    #[test]
    fn generate_has_prefix_and_matching_hash() {
        let (token, hash, prefix) = generate();
        assert!(token.starts_with("tnl_"));
        assert_eq!(hash, sha256_hex(&token));
        assert!(token.starts_with(&prefix));
        assert!(prefix.len() >= 8);
    }
}
