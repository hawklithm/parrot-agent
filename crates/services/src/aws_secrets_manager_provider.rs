//! AWS Secrets Manager provider (PAPERCLIP_MIGRATION_PLAN §4C.2 line 390).
//!
//! Implements `SecretProvider` against the AWS Secrets Manager JSON 1.1 API
//! using the shared SigV4 signer (crates/services/src/aws_sigv4.rs) — no AWS
//! SDK dependency. Configuration via env:
//!   PARROT_AWS_REGION          (default us-east-1)
//!   PARROT_AWS_ACCESS_KEY_ID   (required for non-IMDS deployments)
//!   PARROT_AWS_SECRET_ACCESS_KEY (required for non-IMDS deployments)
//!   PARROT_AWS_SECRETS_ENDPOINT (optional override, e.g. localstack)
//!
//! value_ref format: `aws-sm:<secret-id>` — the caller keeps the AWS secret id.

use crate::aws_sigv4::{hex_sha256, SigV4Signer};
use crate::secret_provider::{ProviderError, SecretProvider};
use async_trait::async_trait;
use models::SecretProviderType;
use serde_json::Value as JsonValue;

const SM_SERVICE: &str = "secretsmanager";

pub struct AwsSecretsManagerProvider {
    region: String,
    access_key: String,
    secret_key: String,
    endpoint: String,
    client: reqwest::Client,
}

impl AwsSecretsManagerProvider {
    pub fn from_env() -> Result<Self, ProviderError> {
        let region = std::env::var("PARROT_AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string());
        let access_key = std::env::var("PARROT_AWS_ACCESS_KEY_ID").map_err(|_| {
            ProviderError::InvalidConfig("PARROT_AWS_ACCESS_KEY_ID is not set".to_string())
        })?;
        let secret_key = std::env::var("PARROT_AWS_SECRET_ACCESS_KEY").map_err(|_| {
            ProviderError::InvalidConfig("PARROT_AWS_SECRET_ACCESS_KEY is not set".to_string())
        })?;
        let endpoint = std::env::var("PARROT_AWS_SECRETS_ENDPOINT").unwrap_or_else(|_| {
            format!("https://secretsmanager.{region}.amazonaws.com")
        });
        Ok(Self {
            region,
            access_key,
            secret_key,
            endpoint,
            client: reqwest::Client::new(),
        })
    }

    /// POST a JSON 1.1 request with SigV4 auth. `target` is the X-Amz-Target
    /// header value (e.g. "secretsmanager.GetSecretValue").
    async fn call(&self, target: &str, body: JsonValue) -> Result<JsonValue, ProviderError> {
        let now = chrono::Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();
        let payload = serde_json::to_vec(&body)
            .map_err(|e| ProviderError::Provider(format!("serialize: {e}")))?;
        let payload_hash = hex_sha256(&payload);
        let host = self
            .endpoint
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("")
            .to_string();
        let canonical_request = format!(
            "POST\n/\n\ncontent-type:application/x-amz-json-1.1\nhost:{host}\nx-amz-date:{amz_date}\nx-amz-target:{target}\n\ncontent-type;host;x-amz-date;x-amz-target\n{payload_hash}"
        );
        let signer = SigV4Signer::new(
            self.access_key.clone(),
            self.secret_key.clone(),
            self.region.clone(),
            SM_SERVICE.to_string(),
        );
        let auth = signer.authorization_header(
            &canonical_request,
            &amz_date,
            &date_stamp,
            "content-type;host;x-amz-date;x-amz-target",
        );
        let resp = self
            .client
            .post(&self.endpoint)
            .header("content-type", "application/x-amz-json-1.1")
            .header("x-amz-date", &amz_date)
            .header("x-amz-target", target)
            .header("authorization", auth)
            .body(payload)
            .send()
            .await
            .map_err(|e| ProviderError::Provider(format!("request failed: {e}")))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| ProviderError::Provider(format!("read response: {e}")))?;
        if !status.is_success() {
            return Err(ProviderError::Provider(format!(
                "AWS Secrets Manager {target} failed: {status} {text}"
            )));
        }
        serde_json::from_str(&text)
            .map_err(|e| ProviderError::Provider(format!("parse response: {e}")))
    }
}

#[async_trait]
impl SecretProvider for AwsSecretsManagerProvider {
    async fn store(
        &self,
        key: &str,
        value: &str,
        _metadata: Option<JsonValue>,
    ) -> Result<String, ProviderError> {
        let secret_id = key.to_string();
        // CreateSecret is not idempotent; treat AlreadyExists as success by
        // falling back to PutSecretValue (versioning a new secret version).
        let create = self
            .call(
                "secretsmanager.CreateSecret",
                serde_json::json!({ "Name": secret_id, "SecretString": value }),
            )
            .await;
        match create {
            Ok(_) => Ok(format!("aws-sm:{secret_id}")),
            Err(e) if e.to_string().contains("ResourceExistsException") => {
                self.call(
                    "secretsmanager.PutSecretValue",
                    serde_json::json!({ "SecretId": secret_id, "SecretString": value }),
                )
                .await?;
                Ok(format!("aws-sm:{secret_id}"))
            }
            Err(e) => Err(e),
        }
    }

    async fn retrieve(&self, value_ref: &str) -> Result<String, ProviderError> {
        let secret_id = value_ref
            .strip_prefix("aws-sm:")
            .ok_or_else(|| ProviderError::Provider("invalid value_ref prefix".to_string()))?;
        let resp = self
            .call(
                "secretsmanager.GetSecretValue",
                serde_json::json!({ "SecretId": secret_id }),
            )
            .await?;
        resp.get("SecretString")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .ok_or_else(|| ProviderError::Provider("SecretString missing in response".to_string()))
    }

    async fn delete(&self, value_ref: &str) -> Result<(), ProviderError> {
        let secret_id = value_ref
            .strip_prefix("aws-sm:")
            .ok_or_else(|| ProviderError::Provider("invalid value_ref prefix".to_string()))?;
        self.call(
            "secretsmanager.DeleteSecret",
            serde_json::json!({ "SecretId": secret_id, "ForceDeleteWithoutRecovery": true }),
        )
        .await?;
        Ok(())
    }

    async fn rotate(&self, value_ref: &str) -> Result<String, ProviderError> {
        // AWS-native rotation: PutSecretValue creates a NEW version of the
        // secret; the caller supplies the rotated value through store().
        // Here we keep the same secret id and let store() version it.
        let secret_id = value_ref
            .strip_prefix("aws-sm:")
            .ok_or_else(|| ProviderError::Provider("invalid value_ref prefix".to_string()))?;
        let rotated = format!("rotated-{}", uuid::Uuid::new_v4());
        self.call(
            "secretsmanager.PutSecretValue",
            serde_json::json!({ "SecretId": secret_id, "SecretString": rotated }),
        )
        .await?;
        Ok(format!("aws-sm:{secret_id}"))
    }

    fn provider_type(&self) -> SecretProviderType {
        SecretProviderType::AwsSecretsManager
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_from_env_requires_credentials() {
        unsafe {
            std::env::remove_var("PARROT_AWS_ACCESS_KEY_ID");
            std::env::remove_var("PARROT_AWS_SECRET_ACCESS_KEY");
            std::env::remove_var("PARROT_AWS_SECRETS_ENDPOINT");
        }
        assert!(
            AwsSecretsManagerProvider::from_env().is_err(),
            "missing credentials must error"
        );
        unsafe {
            std::env::set_var("PARROT_AWS_ACCESS_KEY_ID", "AKIATEST");
            std::env::set_var("PARROT_AWS_SECRET_ACCESS_KEY", "secret");
        }
        let provider = AwsSecretsManagerProvider::from_env().expect("configured env must parse");
        assert_eq!(provider.provider_type(), SecretProviderType::AwsSecretsManager);
        assert_eq!(provider.region, "us-east-1");
        assert!(provider.endpoint.contains("secretsmanager"));
        unsafe {
            std::env::remove_var("PARROT_AWS_ACCESS_KEY_ID");
            std::env::remove_var("PARROT_AWS_SECRET_ACCESS_KEY");
        }
    }

    #[test]
    fn value_ref_validation_rejects_wrong_prefix() {
        let provider = AwsSecretsManagerProvider {
            region: "us-east-1".to_string(),
            access_key: "k".to_string(),
            secret_key: "s".to_string(),
            endpoint: "http://127.0.0.1:1".to_string(),
            client: reqwest::Client::new(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt.block_on(provider.retrieve("local:abc")).unwrap_err();
        assert!(err.to_string().contains("invalid value_ref prefix"));
    }
}
