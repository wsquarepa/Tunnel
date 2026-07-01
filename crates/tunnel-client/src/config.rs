use serde::Deserialize;
use std::collections::HashMap;

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

    pub fn target_addr(&self, name: &str) -> Option<&str> {
        self.targets.get(name).map(String::as_str)
    }
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
}
