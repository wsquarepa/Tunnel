//! Workspace automation. Run via the `cargo xtask` alias.
//!
//! - `cargo xtask test`: the CI gate (rustfmt check, clippy on host + wasm, tests).
//! - `cargo xtask deploy`: provision the D1 database and Worker on Cloudflare,
//!   injecting the real `database_id` into `wrangler.toml` only for the deploy.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

const D1_NAME: &str = "tunnel";

fn main() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("test") => test(),
        Some("deploy") => deploy(),
        _ => {
            eprintln!(
                "usage: cargo xtask <command>\n\
                 \n\
                 commands:\n  \
                 test    run rustfmt check, clippy (host + wasm), and the test suite\n  \
                 deploy  find-or-create the D1 database and Worker, then deploy\n         \
                 (injects the real database_id into wrangler.toml for the deploy only)"
            );
            std::process::exit(2);
        }
    }
}

/// The CI gate. Mirrors the checks a pull request must pass.
fn test() -> Result<()> {
    let root = workspace_root();
    run(
        "rustfmt check",
        "cargo",
        &["fmt", "--all", "--check"],
        &root,
    )?;
    run(
        "clippy (host crates)",
        "cargo",
        &[
            "clippy",
            "-p",
            "tunnel-protocol",
            "-p",
            "tunnel-client",
            "-p",
            "xtask",
            "--",
            "-D",
            "warnings",
        ],
        &root,
    )?;
    run(
        "clippy (worker, wasm)",
        "cargo",
        &[
            "clippy",
            "-p",
            "tunnel-worker",
            "--target",
            "wasm32-unknown-unknown",
            "--",
            "-D",
            "warnings",
        ],
        &root,
    )?;
    run(
        "tests",
        "cargo",
        &[
            "test",
            "-p",
            "tunnel-protocol",
            "-p",
            "tunnel-client",
            "-p",
            "xtask",
        ],
        &root,
    )?;
    println!("\nAll checks passed.");
    Ok(())
}

/// Provision and deploy the Worker.
///
/// `wrangler.toml` intentionally keeps a placeholder `database_id`; the real id
/// is account-specific and never committed. This resolves (or creates) the D1
/// database, patches the id in only for the deploy, then restores the placeholder
/// so the working tree stays clean.
fn deploy() -> Result<()> {
    let worker_dir = workspace_root().join("crates/tunnel-worker");
    let manifest = worker_dir.join("wrangler.toml");

    let id = ensure_database(&worker_dir)?;
    println!("Using D1 database '{D1_NAME}' ({id}).");

    let original = std::fs::read_to_string(&manifest)
        .with_context(|| format!("reading {}", manifest.display()))?;
    std::fs::write(&manifest, set_database_id(&original, &id))
        .with_context(|| format!("writing {}", manifest.display()))?;

    // Restore the committed placeholder regardless of how the deploy ends.
    let deployed = (|| {
        run(
            "apply D1 migrations",
            "npx",
            &["wrangler", "d1", "migrations", "apply", D1_NAME, "--remote"],
            &worker_dir,
        )?;
        run("deploy worker", "npx", &["wrangler", "deploy"], &worker_dir)
    })();
    std::fs::write(&manifest, &original)
        .with_context(|| format!("restoring {}", manifest.display()))?;
    deployed?;

    println!(
        "\nDeployed. If this is a fresh Worker, set the admin secret with:\n  \
         (cd crates/tunnel-worker && npx wrangler secret put ADMIN_SECRET)"
    );
    Ok(())
}

/// Return the D1 database id, creating the database if it does not exist yet.
fn ensure_database(worker_dir: &Path) -> Result<String> {
    if let Some(id) = find_database(worker_dir)? {
        return Ok(id);
    }
    println!("D1 database '{D1_NAME}' not found; creating it.");
    run(
        "create D1 database",
        "npx",
        &["wrangler", "d1", "create", D1_NAME],
        worker_dir,
    )?;
    find_database(worker_dir)?
        .with_context(|| format!("database '{D1_NAME}' not found even after creating it"))
}

/// Look up the account's D1 databases and return the id of `tunnel`, if present.
fn find_database(worker_dir: &Path) -> Result<Option<String>> {
    let stdout = capture("npx", &["wrangler", "d1", "list", "--json"], worker_dir)?;
    let list: serde_json::Value =
        serde_json::from_str(&stdout).context("parsing `wrangler d1 list --json`")?;
    let Some(entries) = list.as_array() else {
        return Ok(None);
    };
    // Field names differ across wrangler versions (uuid/name vs database_id/database_name).
    let id = entries
        .iter()
        .find(|db| {
            field(db, "name") == Some(D1_NAME) || field(db, "database_name") == Some(D1_NAME)
        })
        .and_then(|db| {
            field(db, "uuid")
                .or_else(|| field(db, "database_id"))
                .map(str::to_owned)
        });
    Ok(id)
}

fn field<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(serde_json::Value::as_str)
}

/// Replace the value of the `database_id = "..."` line, leaving everything else
/// (comments, other keys, indentation, trailing newline) untouched.
fn set_database_id(toml: &str, id: &str) -> String {
    let mut out = toml
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with("database_id") && trimmed.contains('=') {
                let indent = &line[..line.len() - trimmed.len()];
                format!("{indent}database_id = \"{id}\"")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if toml.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate is nested in the workspace root")
        .to_path_buf()
}

/// Run a command, streaming its output; error if it exits non-zero.
fn run(desc: &str, program: &str, args: &[&str], cwd: &Path) -> Result<()> {
    println!("\n=== {desc} ===");
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("spawning `{program}`"))?;
    if !status.success() {
        bail!("{desc} failed ({status})");
    }
    Ok(())
}

/// Run a command and capture its stdout; error if it exits non-zero.
fn capture(program: &str, args: &[&str], cwd: &Path) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("spawning `{program}`"))?;
    if !output.status.success() {
        bail!(
            "`{program} {}` failed:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::set_database_id;

    #[test]
    fn replaces_the_placeholder_and_keeps_the_trailing_newline() {
        let src =
            "binding = \"DB\"\ndatabase_id = \"REPLACE_WITH_REAL_ID_FROM_wrangler_d1_create\"\n";
        let out = set_database_id(src, "abc-123");
        assert!(out.contains("database_id = \"abc-123\""));
        assert!(!out.contains("REPLACE_WITH_REAL_ID"));
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn replaces_an_existing_id_without_touching_comments_or_database_name() {
        let src =
            "# database_id is a placeholder\ndatabase_name = \"tunnel\"\ndatabase_id = \"old\"";
        let out = set_database_id(src, "new");
        assert!(out.contains("database_id = \"new\""));
        assert!(out.contains("# database_id is a placeholder"));
        assert!(out.contains("database_name = \"tunnel\""));
        assert!(!out.ends_with('\n'));
    }
}
