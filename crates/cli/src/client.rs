use anyhow::{bail, Result};
use reqwest::blocking::{Client, RequestBuilder};
use std::time::Duration;

use crate::services::ServiceStatus;

#[derive(Debug, Clone)]
pub struct ApiClient {
    pub base_url: String,
    pub api_token: Option<String>,
    client: Client,
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>, api_token: Option<String>) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;
        Ok(Self {
            base_url: base_url.into(),
            api_token,
            client,
        })
    }

    fn request(&self, method: reqwest::Method, path: &str) -> RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.request(method, &url);
        if let Some(token) = &self.api_token {
            req = req.bearer_auth(token);
        }
        req
    }

    fn get(&self, path: &str) -> RequestBuilder { self.request(reqwest::Method::GET, path) }
    fn post(&self, path: &str) -> RequestBuilder { self.request(reqwest::Method::POST, path) }
    fn put(&self, path: &str) -> RequestBuilder { self.request(reqwest::Method::PUT, path) }
    fn delete(&self, path: &str) -> RequestBuilder { self.request(reqwest::Method::DELETE, path) }

    // ── Health ──────────────────────────────────────────────────────

    pub fn health_check(&self) -> Result<ServiceStatus> {
        let resp = self.get("/health").send();
        Ok(match resp {
            Ok(r) if r.status().is_success() => ServiceStatus::Healthy,
            _ => ServiceStatus::Unavailable,
        })
    }

    // ── Auth ────────────────────────────────────────────────────────

    /// Get current session info (works when token is valid)
    pub fn get_session(&self) -> Result<serde_json::Value> {
        let resp = self.get("/api/auth/get-session").send()?;
        if !resp.status().is_success() {
            bail!("auth check failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    // ── Companies ────────────────────────────────────────────────────

    pub fn list_companies(&self) -> Result<serde_json::Value> {
        let resp = self.get("/api/companies").send()?;
        if !resp.status().is_success() {
            bail!("list companies failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    pub fn get_company(&self, company_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/companies/{company_id}")).send()?;
        if !resp.status().is_success() {
            bail!("get company failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    // ── Agents ──────────────────────────────────────────────────────

    pub fn list_agents(&self, company_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/companies/{company_id}/agents")).send()?;
        if !resp.status().is_success() {
            bail!("list agents failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    pub fn get_agent(&self, company_id: &str, agent_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/companies/{company_id}/agents/{agent_id}")).send()?;
        if !resp.status().is_success() {
            bail!("get agent failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    // ── Issues ──────────────────────────────────────────────────────

    pub fn list_issues(&self, company_id: &str, query: Option<&str>) -> Result<serde_json::Value> {
        let mut path = format!("/api/companies/{company_id}/issues");
        if let Some(q) = query {
            path.push_str(&format!("?q={}", urlencode(q)));
        }
        let resp = self.get(&path).send()?;
        if !resp.status().is_success() {
            bail!("list issues failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    pub fn get_issue(&self, company_id: &str, issue_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/companies/{company_id}/issues/{issue_id}")).send()?;
        if !resp.status().is_success() {
            bail!("get issue failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    // ── Goals ───────────────────────────────────────────────────────

    pub fn list_goals(&self, company_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/companies/{company_id}/goals")).send()?;
        if !resp.status().is_success() {
            bail!("list goals failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    // ── Projects ─────────────────────────────────────────────────────

    pub fn list_projects(&self, company_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/companies/{company_id}/projects")).send()?;
        if !resp.status().is_success() {
            bail!("list projects failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    // ── Secrets ──────────────────────────────────────────────────────

    pub fn list_secrets(&self, company_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/companies/{company_id}/secrets")).send()?;
        if !resp.status().is_success() {
            bail!("list secrets failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    // ── Routines ─────────────────────────────────────────────────────

    pub fn list_routines(&self, company_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/companies/{company_id}/routines")).send()?;
        if !resp.status().is_success() {
            bail!("list routines failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    // ── Activity ─────────────────────────────────────────────────────

    pub fn get_activity(&self, company_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/companies/{company_id}/activity")).send()?;
        if !resp.status().is_success() {
            bail!("get activity failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }
}

fn urlencode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urlencode() {
        assert_eq!(urlencode("hello world"), "hello%20world");
        assert_eq!(urlencode("a/b?c"), "a%2Fb%3Fc");
        assert_eq!(urlencode("simple"), "simple");
    }
}
