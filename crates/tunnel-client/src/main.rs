mod config;
mod conn;
mod http_proxy;
mod ws_proxy;

use anyhow::{Context, Result};
use clap::Parser;

#[derive(Parser)]
#[command(name = "tunnel-client", about = "Tunnel agent: forwards public traffic to local ports")]
struct Cli {
    /// Path to the TOML config file.
    #[arg(long, default_value = "tunnel.toml")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    let raw = std::fs::read_to_string(&cli.config)
        .with_context(|| format!("reading config {}", cli.config))?;
    let cfg = config::Config::from_toml(&raw)?;
    let token = cfg
        .resolve_token(std::env::var("TUNNEL_TOKEN").ok())
        .ok_or_else(|| anyhow::anyhow!("no token in config or TUNNEL_TOKEN"))?;
    println!("loaded config for {} ({} targets)", cfg.worker_url, cfg.targets.len());

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    let mut backoff = std::time::Duration::from_millis(500);
    let max_backoff = std::time::Duration::from_secs(30);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                tracing::info!("shutting down");
                break;
            }
            result = conn::run(cfg.clone(), token.clone()) => {
                match result {
                    Ok(()) => tracing::warn!("connection closed; reconnecting in {:?}", backoff),
                    Err(e) => tracing::error!("connection error: {e}; reconnecting in {:?}", backoff),
                }
                tokio::select! {
                    _ = &mut shutdown => break,
                    _ = tokio::time::sleep(backoff) => {}
                }
                backoff = (backoff * 2).min(max_backoff);
            }
        }
    }
    Ok(())
}
