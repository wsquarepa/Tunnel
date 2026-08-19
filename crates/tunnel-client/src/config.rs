use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::collections::HashMap;
use tunnel_protocol::{MAX_ADVERTISED_TARGETS, MAX_TARGET_NAME_BYTES};

#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    pub worker_url: String,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub targets: HashMap<String, String>,
}

impl Config {
    pub fn from_toml(s: &str) -> Result<Config, toml::de::Error> {
        toml::from_str(s)
    }

    /// Env var token wins over the file value.
    pub fn resolve_token(&self, env_token: Option<String>) -> Option<String> {
        env_token.or_else(|| self.token.clone())
    }

    /// The effective config for a run narrowed to `subset`: the same worker and
    /// token, with only the named targets left in the allowlist. Raises an error
    /// naming any target absent from `[targets]`, so a typo fails at startup
    /// instead of becoming a 502 on the public path.
    pub fn restrict(self, subset: &[String]) -> Result<Config> {
        let mut targets = HashMap::with_capacity(subset.len());
        for name in subset {
            let addr = self.targets.get(name).ok_or_else(|| {
                let mut configured: Vec<&str> = self.targets.keys().map(String::as_str).collect();
                configured.sort_unstable();
                anyhow!(
                    "unknown target {name:?} in target subset; config [targets] has: {}",
                    configured.join(", ")
                )
            })?;
            targets.insert(name.clone(), addr.clone());
        }
        Ok(Config { targets, ..self })
    }

    /// Checks that this run's effective targets are advertisable: at least one
    /// target, and within the protocol caps the Durable Object enforces at Hello
    /// by closing the socket, which would otherwise show up only as a reconnect
    /// loop.
    pub fn validate_effective_targets(&self) -> Result<()> {
        if self.targets.is_empty() {
            return Err(anyhow!(
                "no effective targets: add at least one entry to [targets] in the config"
            ));
        }
        if self.targets.len() > MAX_ADVERTISED_TARGETS {
            return Err(anyhow!(
                "{} effective targets exceeds the protocol cap of {MAX_ADVERTISED_TARGETS}; narrow this run with --targets",
                self.targets.len()
            ));
        }
        for name in self.targets.keys() {
            if name.len() > MAX_TARGET_NAME_BYTES {
                return Err(anyhow!(
                    "target name {name:?} is {} bytes, over the protocol cap of {MAX_TARGET_NAME_BYTES}; rename it in [targets]",
                    name.len()
                ));
            }
        }
        Ok(())
    }

    pub fn target_addr(&self, name: &str) -> Option<&str> {
        self.targets.get(name).map(String::as_str)
    }
}

/// Parses a target subset given on the command line or in the environment.
///
/// Accepts one comma-separated value (`"vllm, gradio"`); items are trimmed and
/// repeats collapse to the first occurrence. Raises an error when the value is
/// empty or holds an empty item, since either means the operator meant to name
/// a target and did not.
pub fn parse_target_subset(raw: &str) -> Result<Vec<String>> {
    if raw.trim().is_empty() {
        return Err(anyhow!(
            "empty target subset: --targets/TUNNEL_TARGETS must name at least one configured target"
        ));
    }
    let mut names: Vec<String> = Vec::new();
    for item in raw.split(',') {
        let name = item.trim();
        if name.is_empty() {
            return Err(anyhow!(
                "empty target name in target subset {raw:?}: expected a comma-separated list like \"vllm,gradio\""
            ));
        }
        if !names.iter().any(|seen| seen == name) {
            names.push(name.to_string());
        }
    }
    Ok(names)
}

/// The target subset for one run, from the `--targets` flag and the
/// `TUNNEL_TARGETS` environment value. The flag wins and the environment is
/// ignored when both are set; `None` means the whole configured allowlist is
/// effective. Raises the parse error of whichever source applies.
pub fn resolve_target_subset(flag: Option<&str>, env: Option<&str>) -> Result<Option<Vec<String>>> {
    flag.or(env).map(parse_target_subset).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
worker_url = "wss://tunnel.example.workers.dev"
token = "tnl_filetoken"
[targets]
jupyter = "127.0.0.1:8888"
ollama  = "127.0.0.1:11434"
"#;

    #[test]
    fn parses_full_config() {
        let c = Config::from_toml(SAMPLE).unwrap();
        assert_eq!(c.worker_url, "wss://tunnel.example.workers.dev");
        assert_eq!(c.target_addr("jupyter"), Some("127.0.0.1:8888"));
        assert_eq!(c.target_addr("ollama"), Some("127.0.0.1:11434"));
        assert_eq!(c.target_addr("missing"), None);
    }

    #[test]
    fn env_token_overrides_file() {
        let c = Config::from_toml(SAMPLE).unwrap();
        assert_eq!(
            c.resolve_token(Some("tnl_envtoken".into())).as_deref(),
            Some("tnl_envtoken")
        );
        assert_eq!(c.resolve_token(None).as_deref(), Some("tnl_filetoken"));
    }

    #[test]
    fn missing_token_anywhere_is_none() {
        let c = Config::from_toml(r#"worker_url = "wss://x""#).unwrap();
        assert_eq!(c.resolve_token(None), None);
    }

    #[test]
    fn parses_trimmed_comma_separated_names() {
        assert_eq!(
            parse_target_subset(" jupyter , ollama ").unwrap(),
            vec!["jupyter".to_string(), "ollama".to_string()]
        );
    }

    #[test]
    fn subset_collapses_repeats_and_keeps_first_order() {
        assert_eq!(
            parse_target_subset("ollama,jupyter,ollama").unwrap(),
            vec!["ollama".to_string(), "jupyter".to_string()]
        );
    }

    #[test]
    fn empty_subset_value_is_an_error() {
        assert!(parse_target_subset("").is_err());
        assert!(parse_target_subset("   ").is_err());
    }

    #[test]
    fn empty_subset_item_is_an_error() {
        for raw in ["jupyter,", "jupyter,,ollama", ",jupyter"] {
            let err = parse_target_subset(raw).expect_err("empty item rejected");
            assert!(err.to_string().contains(raw), "{err}");
        }
    }

    #[test]
    fn flag_subset_wins_over_environment() {
        let subset = resolve_target_subset(Some("jupyter"), Some("ollama")).unwrap();
        assert_eq!(subset, Some(vec!["jupyter".to_string()]));
    }

    #[test]
    fn environment_subset_applies_without_a_flag() {
        let subset = resolve_target_subset(None, Some("ollama")).unwrap();
        assert_eq!(subset, Some(vec!["ollama".to_string()]));
    }

    #[test]
    fn no_subset_source_leaves_the_config_whole() {
        assert_eq!(resolve_target_subset(None, None).unwrap(), None);
    }

    #[test]
    fn empty_environment_subset_is_an_error() {
        assert!(resolve_target_subset(None, Some("")).is_err());
    }

    #[test]
    fn restriction_keeps_only_the_named_targets() {
        let c = Config::from_toml(SAMPLE)
            .unwrap()
            .restrict(&["ollama".to_string()])
            .unwrap();
        assert_eq!(c.target_addr("ollama"), Some("127.0.0.1:11434"));
        assert_eq!(c.target_addr("jupyter"), None);
        assert_eq!(c.worker_url, "wss://tunnel.example.workers.dev");
        assert_eq!(c.token.as_deref(), Some("tnl_filetoken"));
    }

    #[test]
    fn restriction_to_a_target_absent_from_the_config_names_it() {
        let err = Config::from_toml(SAMPLE)
            .unwrap()
            .restrict(&["gradio".to_string()])
            .expect_err("unknown target rejected");
        assert!(err.to_string().contains("gradio"), "{err}");
        assert!(err.to_string().contains("jupyter"), "{err}");
    }

    #[test]
    fn effective_targets_pass_the_protocol_caps() {
        let c = Config::from_toml(SAMPLE).unwrap();
        assert!(c.validate_effective_targets().is_ok());
    }

    #[test]
    fn zero_effective_targets_is_an_error() {
        let c = Config::from_toml(r#"worker_url = "wss://x""#).unwrap();
        assert!(c.validate_effective_targets().is_err());
    }

    #[test]
    fn too_many_effective_targets_names_the_count_and_the_cap() {
        let mut c = Config::from_toml(SAMPLE).unwrap();
        c.targets = (0..=MAX_ADVERTISED_TARGETS)
            .map(|i| (format!("t{i}"), "127.0.0.1:1".to_string()))
            .collect();
        let err = c
            .validate_effective_targets()
            .expect_err("oversized advertised set rejected");
        assert!(err.to_string().contains("33"), "{err}");
        assert!(
            err.to_string()
                .contains(&MAX_ADVERTISED_TARGETS.to_string()),
            "{err}"
        );
    }

    #[test]
    fn an_overlong_target_name_is_named_in_the_error() {
        let long = "t".repeat(MAX_TARGET_NAME_BYTES + 1);
        let mut c = Config::from_toml(SAMPLE).unwrap();
        c.targets.insert(long.clone(), "127.0.0.1:1".to_string());
        let err = c
            .validate_effective_targets()
            .expect_err("overlong target name rejected");
        assert!(err.to_string().contains(&long), "{err}");
    }
}
