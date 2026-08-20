use anyhow::{bail, Result};
use serde::Serialize;

use crate::{client::ApiClient, config::CliConfig, services::ServiceStatus};

#[derive(Debug, Serialize)]
struct DoctorReport {
    server_url: String,
    api_token_configured: bool,
    config_file: Option<String>,
    server_status: &'static str,
    database_status: &'static str,
    storage_status: &'static str,
    secrets_status: &'static str,
    status: &'static str,
}

pub fn run_doctor(config: &CliConfig, json_output: bool) -> Result<()> {
    let client = ApiClient::new(config.server_url.clone(), config.api_token.clone());
    let server_status = client.health_check().unwrap_or(ServiceStatus::Unavailable);
    let report = DoctorReport {
        server_url: client.base_url.clone(),
        api_token_configured: client.api_token.is_some(),
        config_file: config
            .config_path
            .as_deref()
            .and_then(|path| path.to_str().map(str::to_owned)),
        server_status: service_status_label(server_status),
        database_status: "not_checked",
        storage_status: "not_checked",
        secrets_status: if config.api_token.is_some() {
            "configured"
        } else {
            "not_configured"
        },
        status: if server_status == ServiceStatus::Healthy {
            "ok"
        } else {
            "failed"
        },
    };
    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("server_url: {}", report.server_url);
        println!(
            "api_token: {}",
            if report.api_token_configured {
                "configured"
            } else {
                "not configured"
            }
        );
        println!(
            "config_file: {}",
            report.config_file.as_deref().unwrap_or("not configured")
        );
        println!("server_status: {}", report.server_status);
        println!(
            "database: not checked (server health endpoint does not expose database diagnostics)"
        );
        println!("storage: not checked");
        println!("secrets: {}", report.secrets_status);
        println!("status: {}", report.status);
    }
    if server_status != ServiceStatus::Healthy {
        bail!("doctor checks failed: server is unavailable")
    }
    Ok(())
}

fn service_status_label(status: ServiceStatus) -> &'static str {
    match status {
        ServiceStatus::Unknown => "not checked",
        ServiceStatus::Healthy => "healthy",
        ServiceStatus::Unavailable => "unavailable",
    }
}
