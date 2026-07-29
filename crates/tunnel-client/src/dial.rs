use anyhow::{anyhow, Result};
use tokio_tungstenite::tungstenite::http::Uri;

/// Where to dial, extracted from the connect URL.
#[derive(Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub tls: bool,
}

/// Decompose a ws:// or wss:// URI into host, port (default 80/443), and
/// whether TLS is required. Errors on any other scheme or a missing host.
pub fn endpoint(uri: &Uri) -> Result<Endpoint> {
    let scheme = uri.scheme_str().unwrap_or_default();
    let tls = match scheme {
        "ws" => false,
        "wss" => true,
        other => {
            return Err(anyhow!(
                "unsupported scheme {other:?} in connect url {uri} (expected ws or wss)"
            ))
        }
    };
    let host = uri
        .host()
        .ok_or_else(|| anyhow!("no host in connect url {uri}"))?
        .to_string();
    let port = uri.port_u16().unwrap_or(if tls { 443 } else { 80 });
    Ok(Endpoint { host, port, tls })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(s: &str) -> Uri {
        s.parse().unwrap()
    }

    #[test]
    fn wss_defaults_to_443_with_tls() {
        assert_eq!(
            endpoint(&uri("wss://tunnel.example.workers.dev/_tunnel/connect")).unwrap(),
            Endpoint {
                host: "tunnel.example.workers.dev".to_string(),
                port: 443,
                tls: true
            }
        );
    }

    #[test]
    fn ws_defaults_to_80_without_tls() {
        assert_eq!(
            endpoint(&uri("ws://localhost/_tunnel/connect")).unwrap(),
            Endpoint {
                host: "localhost".to_string(),
                port: 80,
                tls: false
            }
        );
    }

    #[test]
    fn explicit_port_wins() {
        assert_eq!(
            endpoint(&uri("ws://127.0.0.1:8787/_tunnel/connect")).unwrap(),
            Endpoint {
                host: "127.0.0.1".to_string(),
                port: 8787,
                tls: false
            }
        );
    }

    #[test]
    fn https_scheme_is_rejected() {
        let err = endpoint(&uri("https://x.example/_tunnel/connect"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("unsupported scheme"), "{err}");
        assert!(err.contains("expected ws or wss"), "{err}");
    }
}
