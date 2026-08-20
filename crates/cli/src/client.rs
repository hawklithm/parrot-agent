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
}

