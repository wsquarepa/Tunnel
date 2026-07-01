/// Extract the token from an `Authorization: Bearer <token>` header.
///
/// The scheme match is case-insensitive per RFC 7235; an empty token or any
/// other scheme yields `None`.
pub fn parse_bearer(header: &str) -> Option<&str> {
    let (scheme, rest) = header.split_once(' ')?;
    if scheme.eq_ignore_ascii_case("bearer") && !rest.is_empty() {
        Some(rest)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bearer() {
        assert_eq!(parse_bearer("Bearer tnl_abc"), Some("tnl_abc"));
        assert_eq!(parse_bearer("bearer tnl_abc"), Some("tnl_abc"));
        assert_eq!(parse_bearer("Basic xyz"), None);
        assert_eq!(parse_bearer(""), None);
    }
}
