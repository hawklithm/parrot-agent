use reqwest::{Client, Method, StatusCode};
use serde_json::Value;
use uuid::Uuid;

/// Internal client for reusing the application's existing REST contracts from
/// the migrated Paperclip tool dispatcher.
///
/// This is deliberately separate from the public router layer: it carries the
/// short-lived gateway token and run context explicitly, and returns the
/// upstream status/body without manufacturing a board API key or reimplementing
/// REST handlers inside the MCP layer.
pub(crate) struct PaperclipInternalClient {
    client: Client,
    token: String,
    run_id: Uuid,
    base_url: String,
}

impl PaperclipInternalClient {
    pub(crate) fn new(token: &str, run_id: Uuid) -> Self {
        let configured = std::env::var("PAPERCLIP_API_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:3100/api".to_string());
        let base_url = configured.trim_end_matches('/');
        let base_url = if base_url.ends_with("/api") {
            base_url.to_string()
        } else {
            format!("{base_url}/api")
        };
        Self {
            client: Client::new(),
            token: token.to_string(),
            run_id,
            base_url,
        }
    }

    pub(crate) async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<(StatusCode, Value), String> {
        let path = path.strip_prefix("/api").unwrap_or(path);
        let url = format!("{}{}", self.base_url, path);
        let mut request = self
            .client
            .request(method.clone(), &url)
            .header("x-paperclip-tool-gateway-token", &self.token)
            .header("x-paperclip-run-id", self.run_id.to_string())
            .header("accept", "application/json");
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|error| error.to_string())?;
        let status = response.status();
        let text = response.text().await.map_err(|error| error.to_string())?;
        let value = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        };
        Ok((status, value))
    }
}
