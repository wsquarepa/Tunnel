use std::fmt;

use time::macros::format_description;
use time::OffsetDateTime;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::EnvFilter;

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

/// Install the process-wide log subscriber: `RUST_LOG` (default `info`), no
/// module targets, dimmed `HH:MM:SS` timestamps, colored levels, on stderr.
pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_timer(DimClock)
        .with_writer(std::io::stderr)
        .init();
}

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
        "◈ tunnel  v{version}\n  worker   {worker_url}\n  targets  {list}  ({n})\n",
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
}
