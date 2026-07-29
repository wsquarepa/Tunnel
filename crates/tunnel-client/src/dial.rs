use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Result};
use tokio::net::TcpStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;
use tokio_tungstenite::tungstenite::handshake::client::Request;
use tokio_tungstenite::tungstenite::http::Uri;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

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

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Dial the worker in explicit phases (DNS, TCP, TLS, WS upgrade), logging
/// each phase's outcome and elapsed time at debug so a stall or failure is
/// attributable to a specific phase. Errors name the failed phase and carry
/// the endpoint being dialed.
pub async fn connect(request: Request) -> Result<WsStream> {
    let ep = endpoint(request.uri())?;

    let t = Instant::now();
    let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((ep.host.as_str(), ep.port))
        .await
        .map_err(|e| anyhow!("dns lookup for {}:{} failed: {e}", ep.host, ep.port))?
        .collect();
    if addrs.is_empty() {
        return Err(anyhow!(
            "dns lookup for {}:{} returned no addresses",
            ep.host,
            ep.port
        ));
    }
    tracing::debug!(
        host = %ep.host,
        n = addrs.len(),
        addrs = ?addrs,
        elapsed_ms = t.elapsed().as_millis() as u64,
        "dns resolved"
    );

    let t = Instant::now();
    let mut tcp: Option<TcpStream> = None;
    let mut last_err: Option<std::io::Error> = None;
    for addr in &addrs {
        match TcpStream::connect(addr).await {
            Ok(s) => {
                tcp = Some(s);
                break;
            }
            Err(e) => {
                tracing::debug!(peer = %addr, error = %e, "tcp connect attempt failed");
                last_err = Some(e);
            }
        }
    }
    let Some(tcp) = tcp else {
        return Err(anyhow!(
            "tcp connect to {}:{} failed on all {} addresses: {}",
            ep.host,
            ep.port,
            addrs.len(),
            last_err.map(|e| e.to_string()).unwrap_or_default()
        ));
    };
    tracing::debug!(
        peer = %tcp.peer_addr().map(|a| a.to_string()).unwrap_or_default(),
        local = %tcp.local_addr().map(|a| a.to_string()).unwrap_or_default(),
        elapsed_ms = t.elapsed().as_millis() as u64,
        "tcp connected"
    );

    let stream: MaybeTlsStream<TcpStream> = if ep.tls {
        let t = Instant::now();
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = Arc::new(
            ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        );
        let server_name = ServerName::try_from(ep.host.as_str())
            .map_err(|e| anyhow!("invalid tls server name {:?}: {e}", ep.host))?
            .to_owned();
        let tls = TlsConnector::from(config)
            .connect(server_name, tcp)
            .await
            .map_err(|e| anyhow!("tls handshake with {} failed: {e}", ep.host))?;
        tracing::debug!(
            version = ?tls.get_ref().1.protocol_version(),
            elapsed_ms = t.elapsed().as_millis() as u64,
            "tls established"
        );
        MaybeTlsStream::Rustls(tls)
    } else {
        MaybeTlsStream::Plain(tcp)
    };

    let t = Instant::now();
    let (ws, resp) = tokio_tungstenite::client_async(request, stream)
        .await
        .map_err(|e| anyhow!("websocket upgrade with {}:{} failed: {e}", ep.host, ep.port))?;
    tracing::debug!(
        status = resp.status().as_u16(),
        elapsed_ms = t.elapsed().as_millis() as u64,
        "upgrade complete"
    );
    tracing::trace!(headers = ?resp.headers(), "upgrade response headers");

    Ok(ws)
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
