mod config;

use anyhow::Result;
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
    let raw = std::fs::read_to_string(&cli.config)?;
    let cfg = config::Config::from_toml(&raw)?;
    let token = cfg
        .resolve_token(std::env::var("TUNNEL_TOKEN").ok())
        .ok_or_else(|| anyhow::anyhow!("no token in config or TUNNEL_TOKEN"))?;
    println!("loaded config for {} ({} targets)", cfg.worker_url, cfg.targets.len());
    let _ = token; // wired into the connection loop in Task 2
    Ok(())
}
