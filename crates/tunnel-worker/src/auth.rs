use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

fn mac_hex(secret: &str, msg: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(msg.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Returns `"<issued_at>.<hex hmac>"`.
pub fn sign_session(secret: &str, issued_at: i64) -> String {
    let payload = issued_at.to_string();
    let sig = mac_hex(secret, &payload);
    format!("{payload}.{sig}")
}

/// Verifies signature freshness and integrity in constant time.
pub fn verify_session(secret: &str, cookie: &str, now: i64, max_age_secs: i64) -> bool {
    let Some((payload, sig)) = cookie.split_once('.') else {
        return false;
    };
    let Ok(issued_at) = payload.parse::<i64>() else {
        return false;
    };
    let Some(age) = now.checked_sub(issued_at) else {
        return false;
    };
    if age < 0 || age > max_age_secs {
        return false;
    }
    let expected = mac_hex(secret, payload);
    constant_time_eq(expected.as_bytes(), sig.as_bytes())
}

/// Length-independent equality to avoid timing oracles.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_fresh_cookie() {
        let cookie = sign_session("s3cret", 1000);
        assert!(verify_session("s3cret", &cookie, 1100, 3600));
    }

    #[test]
    fn rejects_wrong_secret() {
        let cookie = sign_session("s3cret", 1000);
        assert!(!verify_session("other", &cookie, 1100, 3600));
    }

    #[test]
    fn rejects_expired_cookie() {
        let cookie = sign_session("s3cret", 1000);
        assert!(!verify_session("s3cret", &cookie, 100000, 3600));
    }

    #[test]
    fn rejects_tampered_cookie() {
        let cookie = sign_session("s3cret", 1000);
        let tampered = cookie.replace('.', ".0");
        assert!(!verify_session("s3cret", &tampered, 1100, 3600));
    }

    #[test]
    fn constant_time_eq_basic() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
