use anyhow::Result;

use crate::services::ServiceStatus;

#[derive(Debug, Clone)]
pub struct ApiClient {
    pub base_url: String,
    pub api_token: Option<String>,
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>, api_token: Option<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_token,
        }
    }

    pub fn health_check(&self) -> Result<ServiceStatus> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()?;
        let mut request = client.get(format!("{}/health", self.base_url));
        if let Some(token) = &self.api_token {
            request = request.bearer_auth(token);
        }
        let response = request.send();
        Ok(match response {
            Ok(response) if response.status().is_success() => ServiceStatus::Healthy,
            _ => ServiceStatus::Unavailable,
        })
    }
}
