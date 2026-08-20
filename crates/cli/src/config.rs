use anyhow::{bail, Result};
use std::{env, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliConfig {
    pub server_url: String,
    pub api_token: Option<String>,
    pub config_path: Option<PathBuf>,
}

impl CliConfig {
    pub fn load() -> Result<Self> {
        let config_path = env::var_os("PARROT_CONFIG").map(PathBuf::from);
        let server_url = env::var("PARROT_SERVER_URL")
            .unwrap_or_else(|_| "http://localhost:3100".to_owned());
        validate_server_url(&server_url)?;

        Ok(Self {
            server_url: server_url.trim_end_matches('/').to_owned(),
            api_token: env::var("PARROT_API_TOKEN").ok().filter(|v| !v.is_empty()),
            config_path,
        })
    }
}

fn validate_server_url(value: &str) -> Result<()> {
    if !(value.starts_with("http://") || value.starts_with("https://")) {
        bail!("PARROT_SERVER_URL must use http or https");
    }
    let host = value
        .strip_prefix("http://")
        .or_else(|| value.strip_prefix("https://"))
        .unwrap_or_default()
        .trim_end_matches('/');
    if host.is_empty() {
        bail!("PARROT_SERVER_URL must include a host");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_server_url;

    #[test]
    fn accepts_http_and_https_urls() {
        assert!(validate_server_url("http://localhost:3100").is_ok());
        assert!(validate_server_url("https://parrot.example").is_ok());
    }

    #[test]
    fn rejects_missing_scheme_or_host() {
        assert!(validate_server_url("localhost:3100").is_err());
        assert!(validate_server_url("https://").is_err());
    }
}
