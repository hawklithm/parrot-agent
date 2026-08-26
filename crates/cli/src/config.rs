use anyhow::{bail, Context, Result};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

const DEFAULT_CONFIG_DIRECTORY: &str = "parrot";
const DEFAULT_CONFIG_FILENAME: &str = "config";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliConfig {
    pub server_url: String,
    pub api_token: Option<String>,
    pub config_path: Option<PathBuf>,
}

impl CliConfig {
    pub fn load() -> Result<Self> {
        Self::load_from(None)
    }

    pub fn load_from(config_path: Option<PathBuf>) -> Result<Self> {
        let config_path = resolve_config_path(config_path);
        let file_values = config_path
            .as_deref()
            .map(read_config_file)
            .transpose()?
            .unwrap_or_default();
        let server_url = env::var("PARROT_SERVER_URL")
            .ok()
            .or_else(|| file_values.get("server_url").cloned())
            .unwrap_or_else(|| "http://localhost:3100".to_owned());
        validate_server_url(&server_url)?;

        Ok(Self {
            server_url: server_url.trim_end_matches('/').to_owned(),
            api_token: env::var("PARROT_API_TOKEN")
                .ok()
                .filter(|v| !v.is_empty())
                .or_else(|| {
                    file_values
                        .get("api_token")
                        .cloned()
                        .filter(|v| !v.is_empty())
                }),
            config_path,
        })
    }

    pub fn save(&self) -> Result<()> {
        let path = self.config_path.as_deref().ok_or_else(|| {
            anyhow::anyhow!("PARROT_CONFIG must be set to save CLI configuration")
        })?;
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let mut contents = format!("server_url={}\n", self.server_url);
        if let Some(token) = &self.api_token {
            contents.push_str(&format!("api_token={}\n", token));
        }
        fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
}

/// Resolve CLI configuration in the same order used by the commands:
/// explicit option, environment override, then the platform default.
pub fn resolve_config_path(explicit_path: Option<PathBuf>) -> Option<PathBuf> {
    resolve_config_path_from(
        explicit_path,
        env::var_os("PARROT_CONFIG").map(PathBuf::from),
        default_config_path(),
    )
}

pub fn default_config_path() -> Option<PathBuf> {
    let appdata = env::var_os("APPDATA").map(PathBuf::from);
    let xdg_config_home = env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from);

    default_config_path_from(
        cfg!(windows),
        appdata.as_deref(),
        xdg_config_home.as_deref(),
        home.as_deref(),
    )
}

fn resolve_config_path_from(
    explicit_path: Option<PathBuf>,
    environment_path: Option<PathBuf>,
    fallback_path: Option<PathBuf>,
) -> Option<PathBuf> {
    explicit_path.or(environment_path).or(fallback_path)
}

fn default_config_path_from(
    windows: bool,
    appdata: Option<&Path>,
    xdg_config_home: Option<&Path>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    let base = if windows {
        appdata.or(home).map(Path::to_path_buf)
    } else {
        xdg_config_home
            .map(Path::to_path_buf)
            .or_else(|| home.map(|path| path.join(".config")))
    }?;

    Some(
        base.join(DEFAULT_CONFIG_DIRECTORY)
            .join(DEFAULT_CONFIG_FILENAME),
    )
}

fn read_config_file(path: &Path) -> Result<std::collections::BTreeMap<String, String>> {
    if !path.exists() {
        return Ok(std::collections::BTreeMap::new());
    }
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut values = std::collections::BTreeMap::new();
    for (line_number, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once('=').ok_or_else(|| {
            anyhow::anyhow!(
                "invalid config line {} in {}",
                line_number + 1,
                path.display()
            )
        })?;
        if !matches!(key.trim(), "server_url" | "api_token") {
            bail!("unknown config key '{}' in {}", key.trim(), path.display());
        }
        values.insert(key.trim().to_owned(), value.trim().to_owned());
    }
    Ok(values)
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
    use super::{
        default_config_path_from, read_config_file, resolve_config_path_from, validate_server_url,
    };
    use std::fs;
    use std::path::{Path, PathBuf};

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

    #[test]
    fn reads_known_config_keys_and_preserves_equals_in_token() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config");
        fs::write(
            &path,
            "server_url=https://example.test\napi_token=abc=def\n",
        )
        .unwrap();
        let values = read_config_file(&path).unwrap();
        assert_eq!(values.get("server_url").unwrap(), "https://example.test");
        assert_eq!(values.get("api_token").unwrap(), "abc=def");
    }

    #[test]
    fn resolves_explicit_environment_and_default_paths_in_order() {
        let explicit = PathBuf::from("explicit/config");
        let environment = PathBuf::from("environment/config");
        let fallback = PathBuf::from("default/config");

        assert_eq!(
            resolve_config_path_from(
                Some(explicit.clone()),
                Some(environment.clone()),
                Some(fallback.clone()),
            ),
            Some(explicit),
        );
        assert_eq!(
            resolve_config_path_from(None, Some(environment.clone()), Some(fallback.clone())),
            Some(environment),
        );
        assert_eq!(
            resolve_config_path_from(None, None, Some(fallback.clone())),
            Some(fallback),
        );
    }

    #[test]
    fn derives_platform_default_config_paths_without_process_environment() {
        let appdata = Path::new("appdata");
        let xdg = Path::new("xdg");
        let home = Path::new("home");

        assert_eq!(
            default_config_path_from(true, Some(appdata), Some(xdg), Some(home)),
            Some(PathBuf::from("appdata").join("parrot").join("config")),
        );
        assert_eq!(
            default_config_path_from(false, Some(appdata), Some(xdg), Some(home)),
            Some(PathBuf::from("xdg").join("parrot").join("config")),
        );
        assert_eq!(
            default_config_path_from(false, None, None, Some(home)),
            Some(
                PathBuf::from("home")
                    .join(".config")
                    .join("parrot")
                    .join("config"),
            ),
        );
    }
}
