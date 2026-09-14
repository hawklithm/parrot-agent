use std::process::Command;

use anyhow::Result;
use serde::Serialize;

use crate::{client::ApiClient, config::CliConfig, services::ServiceStatus};

/// One doctor check result, mirroring Paperclip's `CheckResult` (pass/warn/fail).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CheckResult {
    pub name: String,
    /// "pass" | "warn" | "fail"
    pub status: String,
    pub detail: String,
}

impl CheckResult {
    fn pass(name: &str, detail: impl Into<String>) -> Self {
        Self { name: name.to_string(), status: "pass".to_string(), detail: detail.into() }
    }
    fn warn(name: &str, detail: impl Into<String>) -> Self {
        Self { name: name.to_string(), status: "warn".to_string(), detail: detail.into() }
    }
    fn fail(name: &str, detail: impl Into<String>) -> Self {
        Self { name: name.to_string(), status: "fail".to_string(), detail: detail.into() }
    }
}

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub server_url: String,
    pub config_file: Option<String>,
    pub config_valid: bool,
    pub checks: Vec<CheckResult>,
    pub status: String,
}

/// Probe whether a CLI binary is runnable by invoking `<binary> --version`.
/// A runnable binary is the local precondition for the corresponding adapter's
/// execution lane (Paperclip's `llmCheck` resolves the engine then probes the
/// process). Returns the version string on success.
fn probe_binary(binary: &str) -> Result<String, String> {
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .map_err(|e| format!("{binary} not runnable: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{binary} --version exited {}",
            output.status.code().unwrap_or(-1)
        ));
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version.is_empty() {
        return Err(format!("{binary} --version produced no output"));
    }
    Ok(version)
}

pub fn run_doctor(config: &CliConfig, json_output: bool) -> Result<()> {
    let mut checks: Vec<CheckResult> = Vec::new();

    // Config file present + loads.
    let config_file = config
        .config_path
        .as_ref()
        .and_then(|p| p.to_str().map(str::to_owned));
    let config_valid = config_file.is_some() && config.server_url.starts_with("http");
    checks.push(if config_valid {
        CheckResult::pass("config", "config file present and server_url valid")
    } else if config_file.is_some() {
        CheckResult::warn("config", "config file present but server_url invalid")
    } else {
        CheckResult::warn("config", "no config file resolved")
    });

    // Secrets / auth token.
    checks.push(if config.api_token.is_some() {
        CheckResult::pass("secrets", "api token configured")
    } else {
        CheckResult::warn("secrets", "api token not configured")
    });

    // Server health + derived database status.
    let server_status = match ApiClient::new(config.server_url.clone(), config.api_token.clone()) {
        Ok(client) => client.health_check().unwrap_or(ServiceStatus::Unavailable),
        Err(_) => ServiceStatus::Unknown,
    };
    match server_status {
        ServiceStatus::Healthy => {
            checks.push(CheckResult::pass("server", "server health endpoint reachable"));
            checks.push(CheckResult::pass(
                "database",
                "healthy (via server health)",
            ));
        }
        ServiceStatus::Degraded => {
            checks.push(CheckResult::warn("server", "server reports degraded status"));
            checks.push(CheckResult::warn("database", "not checked (server degraded)"));
        }
        _ => {
            checks.push(CheckResult::fail(
                "server",
                "server health endpoint unreachable",
            ));
            checks.push(CheckResult::fail("database", "not checked (server down)"));
        }
    }

    // Adapter CLI binaries (llmCheck). These are local preconditions; a missing
    // binary is a warning (the server may use a different execution path), not a
    // hard failure.
    for binary in ["claude", "codex"] {
        match probe_binary(binary) {
            Ok(version) => checks.push(CheckResult::pass(
                &format!("adapter:{binary}"),
                format!("{binary} runnable ({version})"),
            )),
            Err(e) => checks.push(CheckResult::warn(&format!("adapter:{binary}"), e)),
        }
    }

    // Keep the individual preflight checks visible, matching Paperclip's
    // checks/* family.  They are warnings when the server can still operate
    // through a different deployment path; only an unreachable server above
    // is a hard failure.
    let jwt_configured = ["PARROT_AGENT_JWT_SECRET", "PAPERCLIP_AGENT_JWT_SECRET"]
        .iter()
        .any(|key| std::env::var(key).map(|value| !value.trim().is_empty()).unwrap_or(false));
    checks.push(if jwt_configured {
        CheckResult::pass("agent-jwt-secret", "agent JWT secret is configured")
    } else {
        CheckResult::warn("agent-jwt-secret", "agent JWT secret is not configured in the environment")
    });

    let deployment_mode = std::env::var("DEPLOYMENT_MODE").unwrap_or_else(|_| "local_trusted".to_owned());
    let deployment_exposure = std::env::var("DEPLOYMENT_EXPOSURE").unwrap_or_else(|_| "private".to_owned());
    checks.push(if matches!(deployment_mode.as_str(), "local_trusted" | "authenticated")
        && matches!(deployment_exposure.as_str(), "private" | "public")
    {
        CheckResult::pass(
            "deployment-auth",
            format!("deployment mode {deployment_mode}, exposure {deployment_exposure}"),
        )
    } else {
        CheckResult::warn(
            "deployment-auth",
            format!("unrecognized deployment mode/exposure: {deployment_mode}/{deployment_exposure}"),
        )
    });

    let executable = std::env::current_exe().ok();
    checks.push(match executable {
        Some(path) if path.is_file() => CheckResult::pass(
            "path-resolver",
            format!("current executable resolved at {}", path.display()),
        ),
        Some(path) => CheckResult::warn(
            "path-resolver",
            format!("current executable path is not a file: {}", path.display()),
        ),
        None => CheckResult::warn("path-resolver", "current executable path could not be resolved"),
    });

    checks.push(check_storage_path());
    checks.push(check_log_path());
    checks.push(check_port(&config.server_url));
    checks.push(check_managed_install());

    let passed = checks.iter().filter(|c| c.status == "pass").count();
    let warned = checks.iter().filter(|c| c.status == "warn").count();
    let failed = checks.iter().filter(|c| c.status == "fail").count();
    let overall = if failed > 0 {
        "fail"
    } else if warned > 0 {
        "warn"
    } else {
        "pass"
    };

    let report = DoctorReport {
        server_url: config.server_url.clone(),
        config_file,
        config_valid,
        checks,
        status: overall.to_string(),
    };

    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("server_url:      {}", report.server_url);
        println!("config_file:     {}", report.config_file.as_deref().unwrap_or("not set"));
        println!("config_valid:    {}", report.config_valid);
        println!();
        for c in &report.checks {
            let icon = match c.status.as_str() {
                "pass" => "✓",
                "warn" => "!",
                _ => "✗",
            };
            println!("  [{icon}] {:<18} {}", c.name, c.detail);
        }
        println!();
        println!(
            "summary:         {} passed, {} warned, {} failed",
            passed, warned, failed
        );
        println!("status:          {}", report.status);
    }

    if failed > 0 {
        anyhow::bail!("doctor checks failed: {} check(s) failed", failed);
    }
    Ok(())
}

fn check_storage_path() -> CheckResult {
    let path = std::env::var_os("PARROT_STORAGE_DIR")
        .or_else(|| std::env::var_os("PARROT_DATA_DIR"))
        .map(std::path::PathBuf::from);
    match path {
        None => CheckResult::warn("storage", "no explicit storage directory configured (using server defaults)"),
        Some(path) if path.is_dir() => CheckResult::pass("storage", format!("storage directory exists: {}", path.display())),
        Some(path) => CheckResult::warn("storage", format!("storage directory does not exist: {}", path.display())),
    }
}

fn check_log_path() -> CheckResult {
    let path = std::env::var_os("PARROT_LOG_DIR").map(std::path::PathBuf::from);
    match path {
        None => CheckResult::warn("log", "PARROT_LOG_DIR is not set (journal/default logging may be in use)"),
        Some(path) if path.is_dir() => CheckResult::pass("log", format!("log directory exists: {}", path.display())),
        Some(path) => CheckResult::warn("log", format!("log directory does not exist: {}", path.display())),
    }
}

fn check_port(server_url: &str) -> CheckResult {
    let Ok(url) = reqwest::Url::parse(server_url) else {
        return CheckResult::warn("port", "server URL could not be parsed");
    };
    let Some(host) = url.host_str() else {
        return CheckResult::warn("port", "server URL has no host");
    };
    let port = url.port_or_known_default().unwrap_or(80);
    let address = format!("{host}:{port}");
    match address
        .parse::<std::net::SocketAddr>()
        .ok()
        .and_then(|address| {
            std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_millis(500)).ok()
        })
    {
        Some(_) => CheckResult::pass("port", format!("server port {address} is reachable")),
        None => CheckResult::warn("port", format!("server port {address} is not reachable")),
    }
}

fn check_managed_install() -> CheckResult {
    let paths = crate::install_store::InstallStorePaths::new();
    match crate::install_store::read_install_manifest(&paths) {
        Ok(Some(manifest)) => CheckResult::pass(
            "managed-install",
            format!("managed install manifest is valid (v{})", manifest.current.version),
        ),
        Ok(None) => CheckResult::warn("managed-install", "no managed install manifest found"),
        Err(error) => CheckResult::warn("managed-install", format!("manifest is invalid: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_fake_binary(dir: &std::path::Path, name: &str, version: &str) {
        let bin = dir.join(name);
        std::fs::write(&bin, format!("#!/bin/sh\necho '{version}'\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bin).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bin, perms).unwrap();
        }
    }

    #[test]
    fn probe_binary_detects_fake_claude() {
        let dir = std::env::temp_dir().join(format!("parrot-dr-fake-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        write_fake_binary(&dir, "claude", "claude 1.2.3 (fake)");
        // Prepend our fake dir to PATH so `claude` resolves to it.
        let original = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{original}", dir.display()));
        let result = probe_binary("claude");
        std::env::set_var("PATH", original);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(result.unwrap(), "claude 1.2.3 (fake)");
    }

    #[test]
    fn probe_binary_reports_missing() {
        let result = probe_binary("parrot-definitely-not-a-real-binary-xyz");
        assert!(result.is_err());
    }
}
