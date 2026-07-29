use std::fmt;
use std::path::Path;

use anyhow::Context;
use time::macros::format_description;
use time::OffsetDateTime;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;

/// Wall-clock `HH:MM:SS` in UTC.
///
/// UTC (not local) is deliberate: `time`'s local-offset lookup is refused in a
/// multi-threaded process (our Tokio runtime), so a local clock would silently
/// render empty. Upgrade path: capture the offset before the runtime starts and
/// switch to `OffsetTime` if local display is ever wanted.
fn hhmmss(now: OffsetDateTime) -> String {
    now.format(format_description!("[hour]:[minute]:[second]"))
        .unwrap_or_default()
}

/// Dims the timestamp when the sink supports ANSI, so the level and message read
/// as the primary content.
struct DimClock;

impl FormatTime for DimClock {
    fn format_time(&self, w: &mut Writer<'_>) -> fmt::Result {
        let s = hhmmss(OffsetDateTime::now_utc());
        if w.has_ansi_escapes() {
            write!(w, "\x1b[2m{s}\x1b[0m")
        } else {
            write!(w, "{s}")
        }
    }
}

/// Install the process-wide log subscriber.
///
/// Terminal (stderr): human-readable, dimmed `HH:MM:SS` timestamps, module
/// targets shown, filtered by `RUST_LOG` (default `info`).
///
/// File (only when `log_file` is given): one JSON object per line, RFC3339
/// timestamps, pinned at trace regardless of `RUST_LOG` so a post-mortem
/// never depends on the terminal verbosity at the time of the incident.
/// Opened in append mode. Raises an error (with path and OS error) when the
/// file cannot be opened; logging to a path the operator asked for is not
/// optional.
pub fn init(log_file: Option<&Path>) -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_timer(DimClock)
        .with_writer(std::io::stderr)
        .with_filter(filter);
    let registry = tracing_subscriber::registry().with(stderr_layer);
    match log_file {
        Some(path) => {
            let file = std::fs::File::options()
                .create(true)
                .append(true)
                .open(path)
                .with_context(|| format!("opening log file {}", path.display()))?;
            let json_layer = tracing_subscriber::fmt::layer()
                .json()
                .with_ansi(false)
                .with_writer(std::sync::Mutex::new(file))
                .with_filter(LevelFilter::TRACE);
            registry.with(json_layer).init();
        }
        None => registry.init(),
    }
    Ok(())
}

/// TUNNEL rendered in the "Alligator2" figlet style; shown at the top of the
/// startup banner.
const ART: &str = r"
  ::::::::::: :::    ::: ::::    ::: ::::    ::: :::::::::: :::
     :+:     :+:    :+: :+:+:   :+: :+:+:   :+: :+:        :+:
    +:+     +:+    +:+ :+:+:+  +:+ :+:+:+  +:+ +:+        +:+
   +#+     +#+    +:+ +#+ +:+ +#+ +#+ +:+ +#+ +#++:++#   +#+
  +#+     +#+    +#+ +#+  +#+#+# +#+  +#+#+# +#+        +#+
 #+#     #+#    #+# #+#   #+#+# #+#   #+#+# #+#        #+#
###      ########  ###    #### ###    #### ########## ##########
";

/// One-time startup summary, printed before the log stream begins. Pure so it can
/// be unit-tested; the caller decides where to write it.
pub fn banner(worker_url: &str, targets: &[String], version: &str) -> String {
    let mut names: Vec<&str> = targets.iter().map(String::as_str).collect();
    names.sort_unstable();
    let list = if names.is_empty() {
        "(none)".to_string()
    } else {
        names.join(", ")
    };
    format!(
        "{ART}\n  v{version}\n  worker   {worker_url}\n  targets  {list}  ({n})\n",
        n = names.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn hhmmss_formats_zero_padded() {
        assert_eq!(hhmmss(datetime!(2026-07-01 09:04:01 UTC)), "09:04:01");
    }

    #[test]
    fn banner_lists_sorted_targets_with_count() {
        let b = banner("wss://x", &["web".into(), "api".into()], "0.1.0");
        assert!(b.contains("v0.1.0"), "{b}");
        assert!(b.contains("wss://x"), "{b}");
        assert!(b.contains("api, web"), "{b}");
        assert!(b.contains("(2)"), "{b}");
    }

    #[test]
    fn banner_handles_no_targets() {
        let b = banner("wss://x", &[], "0.1.0");
        assert!(b.contains("(none)"), "{b}");
        assert!(b.contains("(0)"), "{b}");
    }

    #[test]
    fn banner_includes_ascii_art() {
        let b = banner("wss://x", &[], "0.1.0");
        assert!(b.contains(":+:"), "{b}");
    }
}
