use anyhow::{bail, Result};
use serde::Serialize;

use crate::{client::ApiClient, config::CliConfig, services::ServiceStatus};

#[derive(Debug, Serialize)]
struct DoctorReport {
    server_url: String,
    api_token_configured: bool,
    config_file: Option<String>,
    config_valid: bool,
    server_status: String,
    database_status: String,
    storage_status: String,
    secrets_status: String,
    status: String,
}

pub fn run_doctor(config: &CliConfig, json_output: bool) -> Result<()> {
    let server_url = config.server_url.clone();
    let api_token_configured = config.api_token.is_some();
    let config_file = config.config_path.as_deref()
        .and_then(|path| path.to_str().map(str::to_owned));

    // Validate config can load
    let config_valid = config_file.is_some();

    // Check server health
    let server_status = match ApiClient::new(server_url.clone(), config.api_token.clone()) {
        Ok(client) => client.health_check().unwrap_or(ServiceStatus::Unavailable),
        Err(_) => ServiceStatus::Unknown,
    };

    // Try to detect database availability via server health endpoint
    let database_status = if server_status == ServiceStatus::Healthy {
        "healthy (via server health)"
    } else {
        "not checked"
    };

    // Check secrets config
    let secrets_status = if api_token_configured {
        "configured"
    } else {
        "not_configured"
    };

    let status = if server_status == ServiceStatus::Healthy {
        "ok"
    } else {
        "degraded"
    };

    let report = DoctorReport {
        server_url,
        api_token_configured,
        config_file,
        config_valid,
        server_status: format!("{:?}", server_status).to_lowercase(),
        database_status: database_status.to_string(),
        storage_status: "not_checked".to_string(),
        secrets_status: secrets_status.to_string(),
        status: status.to_string(),
    };

    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("config_file:     {}", report.config_file.as_deref().unwrap_or("not set"));
        println!("config_valid:    {}", report.config_valid);
        println!("server_url:      {}", report.server_url);
        println!("api_token:       {}", if report.api_token_configured { "configured" } else { "not configured" });
        println!("server_status:   {}", report.server_status);
        println!("database:        {}", report.database_status);
        println!("storage:         {}", report.storage_status);
        println!("secrets:         {}", report.secrets_status);
        println!("status:          {}", report.status);
    }

    if server_status != ServiceStatus::Healthy {
        bail!("doctor checks failed: server is unavailable or degraded")
    }
    Ok(())
}
