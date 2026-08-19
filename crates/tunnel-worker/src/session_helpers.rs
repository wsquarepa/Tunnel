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

/// One live control socket as dispatch sees it: its connection id, the targets
/// it advertised in Hello (`None` until Hello completes, which makes the socket
/// capable of nothing), its in-flight stream count, and the socket handle.
///
/// The handle is generic so the selection logic stays free of Worker runtime
/// types and runs on the host; the Durable Object uses `worker::WebSocket`.
pub struct PoolSocket<H> {
    pub conn: u64,
    pub advertised: Option<Vec<String>>,
    pub active_streams: usize,
    pub handle: H,
}

/// Pool sockets ordered by in-flight stream count, fewest first, with the
/// connection id as the tiebreak so the order is deterministic.
pub fn sort_by_load<H>(mut sockets: Vec<PoolSocket<H>>) -> Vec<PoolSocket<H>> {
    sockets.sort_by_key(|s| (s.active_streams, s.conn));
    sockets
}

/// The capable sockets for `target` (live sockets whose advertised targets
/// contain it), least loaded first. Empty when no socket advertised the target.
pub fn capable_by_load<H>(sockets: Vec<PoolSocket<H>>, target: &str) -> Vec<PoolSocket<H>> {
    let capable = sockets
        .into_iter()
        .filter(|s| {
            s.advertised
                .as_ref()
                .is_some_and(|names| names.iter().any(|n| n == target))
        })
        .collect();
    sort_by_load(capable)
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

    fn socket(conn: u64, advertised: Option<&[&str]>, active_streams: usize) -> PoolSocket<()> {
        PoolSocket {
            conn,
            advertised: advertised.map(|a| a.iter().map(|s| s.to_string()).collect()),
            active_streams,
            handle: (),
        }
    }

    fn conns<H>(sockets: &[PoolSocket<H>]) -> Vec<u64> {
        sockets.iter().map(|s| s.conn).collect()
    }

    #[test]
    fn capable_sockets_only_least_loaded_first() {
        let pool = vec![
            socket(3, Some(&["vllm", "gradio"]), 2),
            socket(1, Some(&["gradio"]), 0),
            socket(2, Some(&["vllm"]), 0),
            socket(4, Some(&["vllm"]), 1),
        ];
        assert_eq!(conns(&capable_by_load(pool, "vllm")), vec![2, 4, 3]);
    }

    #[test]
    fn equal_load_breaks_ties_by_conn() {
        let pool = vec![
            socket(9, Some(&["vllm"]), 1),
            socket(5, Some(&["vllm"]), 1),
            socket(7, Some(&["vllm"]), 1),
        ];
        assert_eq!(conns(&capable_by_load(pool, "vllm")), vec![5, 7, 9]);
    }

    #[test]
    fn socket_without_attachment_is_capable_of_nothing() {
        let pool = vec![socket(1, None, 0), socket(2, Some(&[]), 0)];
        assert!(capable_by_load(pool, "vllm").is_empty());
    }

    #[test]
    fn no_capable_socket_yields_empty() {
        let pool = vec![socket(1, Some(&["gradio"]), 0)];
        assert!(capable_by_load(pool, "vllm").is_empty());
    }

    #[test]
    fn sort_by_load_keeps_every_socket() {
        let pool = vec![socket(2, None, 3), socket(1, Some(&["vllm"]), 0)];
        assert_eq!(conns(&sort_by_load(pool)), vec![1, 2]);
    }
}
