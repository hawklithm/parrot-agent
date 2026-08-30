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

    // ── Generic JSON body senders ──────────────────────────────────

    fn send_json(&self, req: RequestBuilder, body: Option<&serde_json::Value>) -> Result<serde_json::Value> {
        let req = if let Some(body) = body {
            req.header("content-type", "application/json").json(body)
        } else {
            req
        };
        let resp = req.send()?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            bail!("request failed: HTTP {}: {}", status, text);
        }
        if text.trim().is_empty() {
            Ok(serde_json::Value::Null)
        } else {
            Ok(serde_json::from_str(&text)?)
        }
    }

    // ── Company mutations ──────────────────────────────────────────

    pub fn create_company(&self, body: &serde_json::Value) -> Result<serde_json::Value> {
        self.send_json(self.post("/api/companies"), Some(body))
    }

    pub fn delete_company(&self, company_id: &str) -> Result<serde_json::Value> {
        self.send_json(self.delete(&format!("/api/companies/{company_id}")), None)
    }

    pub fn export_company(&self, company_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/companies/{company_id}/export")).send()?;
        if !resp.status().is_success() {
            bail!("export company failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    pub fn import_company(&self, body: &serde_json::Value) -> Result<serde_json::Value> {
        self.send_json(self.post("/api/companies/import"), Some(body))
    }

    // ── Approvals ──────────────────────────────────────────────────

    pub fn list_approvals(&self, company_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/companies/{company_id}/approvals")).send()?;
        if !resp.status().is_success() {
            bail!("list approvals failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    pub fn get_approval(&self, approval_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/approvals/{approval_id}")).send()?;
        if !resp.status().is_success() {
            bail!("get approval failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    pub fn approve_approval(&self, approval_id: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
        self.send_json(self.post(&format!("/api/approvals/{approval_id}/approve")), Some(body))
    }

    pub fn reject_approval(&self, approval_id: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
        self.send_json(self.post(&format!("/api/approvals/{approval_id}/reject")), Some(body))
    }

    pub fn resubmit_approval(&self, approval_id: &str, body: Option<&serde_json::Value>) -> Result<serde_json::Value> {
        self.send_json(self.post(&format!("/api/approvals/{approval_id}/resubmit")), body)
    }

    // ── Pipelines ──────────────────────────────────────────────────

    pub fn list_pipelines(&self, company_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/companies/{company_id}/pipelines")).send()?;
        if !resp.status().is_success() {
            bail!("list pipelines failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    pub fn get_pipeline(&self, pipeline_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/pipelines/{pipeline_id}")).send()?;
        if !resp.status().is_success() {
            bail!("get pipeline failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    // ── Skills ─────────────────────────────────────────────────────

    pub fn list_skills(&self) -> Result<serde_json::Value> {
        let resp = self.get("/api/skills/index").send()?;
        if !resp.status().is_success() {
            bail!("list skills failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    pub fn get_skill(&self, skill_name: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/skills/{}", urlencode(skill_name))).send()?;
        if !resp.status().is_success() {
            bail!("get skill failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    // ── Teams ──────────────────────────────────────────────────────

    pub fn list_teams_catalog(&self) -> Result<serde_json::Value> {
        let resp = self.get("/api/teams/catalog").send()?;
        if !resp.status().is_success() {
            bail!("list teams catalog failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    // ── Plugins ────────────────────────────────────────────────────

    pub fn list_plugins(&self) -> Result<serde_json::Value> {
        let resp = self.get("/api/plugins").send()?;
        if !resp.status().is_success() {
            bail!("list plugins failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    pub fn install_plugin(&self, body: &serde_json::Value) -> Result<serde_json::Value> {
        self.send_json(self.post("/api/plugins/install"), Some(body))
    }

    pub fn enable_plugin(&self, plugin_id: &str) -> Result<serde_json::Value> {
        self.send_json(self.post(&format!("/api/plugins/{plugin_id}/enable")), None)
    }

    pub fn disable_plugin(&self, plugin_id: &str) -> Result<serde_json::Value> {
        self.send_json(self.post(&format!("/api/plugins/{plugin_id}/disable")), None)
    }

    // ── Dashboard ──────────────────────────────────────────────────

    pub fn get_dashboard(&self, company_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/companies/{company_id}/dashboard")).send()?;
        if !resp.status().is_success() {
            bail!("get dashboard failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    // ── Costs ──────────────────────────────────────────────────────

    pub fn get_cost_summary(&self, company_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/companies/{company_id}/costs/summary")).send()?;
        if !resp.status().is_success() {
            bail!("get cost summary failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    // ── Feedback ───────────────────────────────────────────────────

    pub fn get_feedback_trace(&self, trace_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/feedback-traces/{trace_id}")).send()?;
        if !resp.status().is_success() {
            bail!("get feedback trace failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    // ── Access (org chart / teams) ─────────────────────────────────

    pub fn get_org_chart(&self, company_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/companies/{company_id}/org-chart")).send()?;
        if !resp.status().is_success() {
            bail!("get org chart failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    // ── Workspaces ─────────────────────────────────────────────────

    pub fn list_workspaces(&self, company_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/companies/{company_id}/execution-workspaces")).send()?;
        if !resp.status().is_success() {
            bail!("list workspaces failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    pub fn get_workspace(&self, workspace_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/execution-workspaces/{workspace_id}")).send()?;
        if !resp.status().is_success() {
            bail!("get workspace failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    // ── Runs ───────────────────────────────────────────────────────

    pub fn list_issue_runs(&self, issue_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/issues/{issue_id}/runs")).send()?;
        if !resp.status().is_success() {
            bail!("list issue runs failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    pub fn get_run(&self, run_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/heartbeat-runs/{run_id}")).send()?;
        if !resp.status().is_success() {
            bail!("get run failed: HTTP {}", resp.status());
        }
        Ok(resp.json()?)
    }

    // ── Channels ───────────────────────────────────────────────────

    pub fn list_channels(&self, company_id: &str) -> Result<serde_json::Value> {
        let resp = self.get(&format!("/api/companies/{company_id}/channels")).send()?;
        if !resp.status().is_success() {
            bail!("list channels failed: HTTP {}", resp.status());
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
