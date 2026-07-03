mod config;
mod conn;
mod http_proxy;
mod logging;
mod ws_proxy;

use anyhow::{Context, Result};
use clap::Parser;
use std::time::{Duration, Instant};

const INITIAL_BACKOFF: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
// A session that stayed up at least this long counts as successful, so the
// reconnect after it restarts from INITIAL_BACKOFF instead of continuing to
// grow. A connection that flaps faster than this keeps escalating, which
// throttles hammering a broken endpoint. Ceiling: sessions shorter than this
// but still real (e.g. a genuine 3s connection) are treated as failures.
const STABLE_CONNECTION: Duration = Duration::from_secs(5);

/// Delay before the next reconnect, given the `previous` delay and how long the
/// session that just ended stayed up. A successful session resets the delay to
/// `INITIAL_BACKOFF`; a fast failure doubles it up to `MAX_BACKOFF`.
fn reconnect_backoff(previous: Duration, uptime: Duration) -> Duration {
    if uptime >= STABLE_CONNECTION {
        INITIAL_BACKOFF
    } else {
        (previous * 2).min(MAX_BACKOFF)
    }
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
}

#[tokio::main]
async fn main() -> Result<()> {
    logging::init();
    let cli = Cli::parse();
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

    loop {
        let connected_at = Instant::now();
        tokio::select! {
            _ = &mut shutdown => {
                tracing::info!("shutting down");
                break;
            }
            result = conn::run(cfg.clone(), token.clone()) => {
                backoff = reconnect_backoff(backoff, connected_at.elapsed());
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
    use super::{reconnect_backoff, INITIAL_BACKOFF, MAX_BACKOFF, STABLE_CONNECTION};
    use std::time::Duration;

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
}
