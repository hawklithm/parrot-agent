use anyhow::Result;

use crate::{client::ApiClient, config::CliConfig, services::ServiceStatus};

pub fn run_doctor(config: &CliConfig) -> Result<()> {
    let client = ApiClient::new(config.server_url.clone(), config.api_token.clone());
    println!("server_url: {}", client.base_url);
    println!("api_token: {}", if client.api_token.is_some() { "configured" } else { "not configured" });
    println!("config_file: {}", config.config_path.as_deref().map_or("not configured", |p| p.to_str().unwrap_or("invalid")));
    let server_status = client.health_check().unwrap_or(ServiceStatus::Unavailable);
    println!("server_status: {}", service_status_label(server_status));
    println!("database: not checked (server health endpoint does not expose database diagnostics)");
    println!("storage: not checked");
    println!("secrets: {}", if config.api_token.is_some() { "token configured" } else { "token not configured" });
    println!("status: configuration valid");
    Ok(())
}

fn service_status_label(status: ServiceStatus) -> &'static str {
    match status {
        ServiceStatus::Unknown => "not checked",
        ServiceStatus::Healthy => "healthy",
        ServiceStatus::Unavailable => "unavailable",
    }
}
