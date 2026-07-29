mod config;
mod conn;
mod dial;
mod http_proxy;
mod liveness;
mod logging;
mod ws_proxy;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tracing::Instrument;

const INITIAL_BACKOFF: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
// A session that stayed up at least this long counts as successful, so the
// reconnect after it restarts from INITIAL_BACKOFF instead of continuing to
// grow. A connection that flaps faster than this keeps escalating, which
// throttles hammering a broken endpoint. Ceiling: sessions shorter than this
// but still real (e.g. a genuine 3s connection) are treated as failures.
const STABLE_CONNECTION: Duration = Duration::from_secs(5);

/// Delay before the next reconnect, given the `previous` delay and how long the
/// session that just ended stayed up, counted from its HelloAck. A successful
/// session resets the delay to `INITIAL_BACKOFF`; a fast failure, including a
/// socket that opened but never acked, doubles it up to `MAX_BACKOFF`.
fn reconnect_backoff(previous: Duration, uptime: Duration) -> Duration {
    if uptime >= STABLE_CONNECTION {
        INITIAL_BACKOFF
    } else {
        (previous * 2).min(MAX_BACKOFF)
    }
}

/// How long the session that just ended was usable, measured from the instant
/// its HelloAck arrived. A cell that was never set means the handshake never
/// completed, which is worth no uptime at all however long the socket was open.
fn session_uptime(acked_at: &OnceLock<Instant>) -> Duration {
    acked_at
        .get()
        .map(Instant::elapsed)
        .unwrap_or(Duration::ZERO)
}

#[derive(Parser)]
#[command(
    name = "tunnel-client",
    about = "Tunnel agent: forwards public traffic to local ports"
)]
struct Cli {
    /// Path to the TOML config file.
    #[arg(long, default_value = "tunnel.toml")]
    config: String,

    /// Also write logs to this file as JSON lines, at trace verbosity.
    #[arg(long, value_name = "FILE")]
    log: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    logging::init(cli.log.as_deref())?;
    let raw = std::fs::read_to_string(&cli.config)
        .with_context(|| format!("reading config {}", cli.config))?;
    let cfg = config::Config::from_toml(&raw)?;
    let token = cfg
        .resolve_token(std::env::var("TUNNEL_TOKEN").ok())
        .ok_or_else(|| anyhow::anyhow!("no token in config or TUNNEL_TOKEN"))?;
    let target_names: Vec<String> = cfg.targets.keys().cloned().collect();
    eprint!(
        "{}",
        logging::banner(&cfg.worker_url, &target_names, env!("CARGO_PKG_VERSION"))
    );

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    let mut backoff = INITIAL_BACKOFF;
    let mut attempt: u64 = 0;

    loop {
        attempt += 1;
        let acked_at: Arc<OnceLock<Instant>> = Arc::new(OnceLock::new());
        let span = tracing::info_span!("conn", attempt, session_id = tracing::field::Empty);
        tokio::select! {
            _ = &mut shutdown => {
                tracing::info!("shutting down");
                break;
            }
            result = conn::run(cfg.clone(), token.clone(), acked_at.clone()).instrument(span) => {
                backoff = reconnect_backoff(backoff, session_uptime(&acked_at));
                match result {
                    Ok(()) => tracing::warn!("connection closed; reconnecting in {:?}", backoff),
                    Err(e) => tracing::error!("connection error: {e}; reconnecting in {:?}", backoff),
                }
                tokio::select! {
                    _ = &mut shutdown => break,
                    _ = tokio::time::sleep(backoff) => {}
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        reconnect_backoff, session_uptime, INITIAL_BACKOFF, MAX_BACKOFF, STABLE_CONNECTION,
    };
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    #[test]
    fn stable_session_resets_backoff() {
        let escalated = Duration::from_secs(16);
        assert_eq!(
            reconnect_backoff(escalated, STABLE_CONNECTION),
            INITIAL_BACKOFF
        );
    }

    #[test]
    fn fast_failure_doubles_up_to_max() {
        assert_eq!(
            reconnect_backoff(INITIAL_BACKOFF, Duration::ZERO),
            INITIAL_BACKOFF * 2
        );
        assert_eq!(
            reconnect_backoff(Duration::from_secs(20), Duration::ZERO),
            MAX_BACKOFF
        );
    }

    #[test]
    fn never_acked_session_escalates_backoff() {
        let acked_at: OnceLock<Instant> = OnceLock::new();
        assert_eq!(session_uptime(&acked_at), Duration::ZERO);
        assert_eq!(
            reconnect_backoff(INITIAL_BACKOFF, session_uptime(&acked_at)),
            INITIAL_BACKOFF * 2
        );
    }

    #[test]
    fn acked_session_uptime_counts_from_the_ack() {
        let acked_at: OnceLock<Instant> = OnceLock::new();
        acked_at
            .set(Instant::now() - STABLE_CONNECTION - Duration::from_secs(1))
            .expect("cell is empty");
        assert!(session_uptime(&acked_at) >= STABLE_CONNECTION);
        assert_eq!(
            reconnect_backoff(Duration::from_secs(16), session_uptime(&acked_at)),
            INITIAL_BACKOFF
        );
    }
}
