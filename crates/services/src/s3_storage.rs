//! S3-compatible storage provider (PAPERCLIP_MIGRATION_PLAN §4C.1 line 380).
//!
//! Implements `StorageService` against any S3-compatible endpoint (AWS S3,
//! MinIO, R2, …) using SigV4-signed REST calls over reqwest — no AWS SDK
//! dependency. Configuration via env:
//!   PARROT_S3_ENDPOINT       (default https://s3.amazonaws.com)
//!   PARROT_S3_REGION         (default us-east-1)
//!   PARROT_S3_BUCKET         (required)
//!   PARROT_S3_ACCESS_KEY     (required)
//!   PARROT_S3_SECRET_KEY     (required)
//!   PARROT_S3_PATH_STYLE     ("true" for MinIO-style path addressing, default false)
//!
//! Object keys are the same `company_id/namespace/...` keys used by the local
//! provider, so company isolation is preserved on the S3 side.

use crate::asset_storage::{build_object_key, PutFileRequest, StoredObject, StorageService};
use crate::aws_sigv4::{hex_hmac, hex_sha256, hmac_sha256, urlencode, SigV4Signer};
use crate::errors::{ServiceError, ServiceResult};
use async_trait::async_trait;
use uuid::Uuid;

/// S3 provider configuration.
#[derive(Debug, Clone)]
pub struct S3Config {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub path_style: bool,
}

impl S3Config {
    /// Load from environment. Returns Err when bucket or credentials are absent.
    pub fn from_env() -> Result<Self, String> {
        let bucket = std::env::var("PARROT_S3_BUCKET").map_err(|_| {
            "PARROT_S3_BUCKET is not set; S3 storage provider is not configured".to_string()
        })?;
        let access_key = std::env::var("PARROT_S3_ACCESS_KEY").map_err(|_| {
            "PARROT_S3_ACCESS_KEY is not set; S3 storage provider is not configured".to_string()
        })?;
        let secret_key = std::env::var("PARROT_S3_SECRET_KEY").map_err(|_| {
            "PARROT_S3_SECRET_KEY is not set; S3 storage provider is not configured".to_string()
        })?;
        Ok(Self {
            endpoint: std::env::var("PARROT_S3_ENDPOINT")
                .unwrap_or_else(|_| "https://s3.amazonaws.com".to_string()),
            region: std::env::var("PARROT_S3_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
            bucket,
            access_key,
            secret_key,
            path_style: std::env::var("PARROT_S3_PATH_STYLE")
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
        })
    }
}

/// S3-compatible storage backend.
pub struct S3StorageService {
    config: S3Config,
    client: reqwest::Client,
}

impl S3StorageService {
    pub fn new(config: S3Config) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    pub fn from_env() -> Result<Self, String> {
        Ok(Self::new(S3Config::from_env()?))
    }

    fn object_url(&self, object_key: &str) -> String {
        let base = self.config.endpoint.trim_end_matches('/');
        if self.config.path_style {
            format!("{base}/{}/{}", self.config.bucket, object_key)
        } else {
            let host = format!("{}.{}", self.config.bucket, base.trim_start_matches("https://").trim_start_matches("http://"));
            let scheme = if base.starts_with("https://") { "https" } else { "http" };
            format!("{scheme}://{host}/{object_key}")
        }
    }

    /// Build a SigV4-signed GET (presigned URL) with the given expiry.
    pub fn presigned_get_url(&self, object_key: &str, expires_secs: u32) -> String {
        let now = chrono::Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();
        let host = self.host_for(object_key);
        let query = format!(
            "X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential={}%2F{date_stamp}%2F{}%2Fs3%2Faws4_request&X-Amz-Date={amz_date}&X-Amz-Expires={expires_secs}&X-Amz-SignedHeaders=host",
            urlencode(&self.config.access_key),
            self.config.region,
        );
        let canonical_request = format!(
            "GET\n/{}\n{query}\nhost:{host}\n\nhost\nUNSIGNED-PAYLOAD",
            object_key
        );
        let signature = self.signature(&canonical_request, &amz_date, &date_stamp);
        let base = self.object_url(object_key);
        format!("{base}?{query}&X-Amz-Signature={signature}")
    }

    fn host_for(&self, object_key: &str) -> String {
        // host header must match the request URL host
        let url = self.object_url(object_key);
        url.split("://")
            .nth(1)
            .unwrap_or(&url)
            .split('/')
            .next()
            .unwrap_or("")
            .to_string()
    }

    fn signer(&self) -> SigV4Signer {
        SigV4Signer::new(
            self.config.access_key.clone(),
            self.config.secret_key.clone(),
            self.config.region.clone(),
            "s3".to_string(),
        )
    }

    fn signature(&self, canonical_request: &str, amz_date: &str, date_stamp: &str) -> String {
        self.signer().signature(canonical_request, amz_date, date_stamp)
    }

    async fn send_signed(
        &self,
        method: reqwest::Method,
        object_key: &str,
        body: Option<&[u8]>,
        content_type: Option<&str>,
    ) -> ServiceResult<reqwest::Response> {
        let now = chrono::Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();
        let host = self.host_for(object_key);
        let payload_hash = match body {
            Some(bytes) => hex_sha256(bytes),
            None => hex_sha256(b""),
        };
        let mut headers = format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}");
        let mut signed_headers = "host;x-amz-content-sha256;x-amz-date".to_string();
        if let Some(ct) = content_type {
            headers.push_str(&format!("\ncontent-type:{ct}"));
            signed_headers.push_str(";content-type");
        }
        let canonical_request = format!(
            "{}\n/{}\n\n{}\n\n{}\n{}",
            method.as_str(),
            object_key,
            headers,
            signed_headers,
            payload_hash,
        );
        let signature = self.signature(&canonical_request, &amz_date, &date_stamp);

        let mut req = self
            .client
            .request(method.clone(), self.object_url(object_key))
            .header("x-amz-date", &amz_date)
            .header("x-amz-content-sha256", &payload_hash)
            .header(
                "authorization",
                format!(
                    "AWS4-HMAC-SHA256 Credential={}/{date_stamp}/{}/s3/aws4_request, SignedHeaders={signed_headers}, Signature={signature}",
                    self.config.access_key, self.config.region,
                ),
            );
        if let Some(ct) = content_type {
            req = req.header("content-type", ct);
        }
        if let Some(bytes) = body {
            req = req.body(bytes.to_vec());
        }
        req.send()
            .await
            .map_err(|e| ServiceError::Internal(format!("S3 request failed: {e}")))
    }
}

#[async_trait]
impl StorageService for S3StorageService {
    async fn put_file(&self, req: PutFileRequest) -> ServiceResult<StoredObject> {
        if req.body.is_empty() {
            return Err(ServiceError::Unprocessable("File is empty".into()));
        }
        let object_key = build_object_key(
            req.company_id,
            &req.namespace,
            req.original_filename.as_deref(),
        );
        let resp = self
            .send_signed(
                reqwest::Method::PUT,
                &object_key,
                Some(&req.body),
                Some(&req.content_type),
            )
            .await?;
        if !resp.status().is_success() {
            return Err(ServiceError::Internal(format!(
                "S3 put failed: {} {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            )));
        }
        Ok(StoredObject {
            provider: "s3".to_string(),
            object_key,
            content_type: req.content_type,
            byte_size: req.body.len() as i64,
            sha256: hex_sha256(&req.body),
            original_filename: req.original_filename,
        })
    }

    async fn get_object(&self, company_id: Uuid, object_key: &str) -> ServiceResult<Vec<u8>> {
        if !crate::asset_storage::object_key_belongs_to_company(object_key, company_id) {
            return Err(ServiceError::NotFound("object not found".into()));
        }
        let resp = self
            .send_signed(reqwest::Method::GET, object_key, None, None)
            .await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ServiceError::NotFound("object not found in storage".into()));
        }
        if !resp.status().is_success() {
            return Err(ServiceError::Internal(format!(
                "S3 get failed: {} {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            )));
        }
        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| ServiceError::Internal(format!("S3 read failed: {e}")))
    }

    async fn delete_object(&self, company_id: Uuid, object_key: &str) -> ServiceResult<()> {
        if !crate::asset_storage::object_key_belongs_to_company(object_key, company_id) {
            return Ok(());
        }
        let resp = self
            .send_signed(reqwest::Method::DELETE, object_key, None, None)
            .await?;
        if resp.status().is_success() || resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        Err(ServiceError::Internal(format!(
            "S3 delete failed: {} {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sigv4_signature_matches_aws_test_vector() {
        // AWS SigV4 documentation example: GET /test.txt on examplebucket,
        // 2013-05-24T00:00:00Z, us-east-1. The documented canonical request
        // (host + range + x-amz-* headers) must produce the documented final
        // signature when passed through our signing chain.
        let svc = S3StorageService::new(S3Config {
            endpoint: "https://examplebucket.s3.amazonaws.com".to_string(),
            region: "us-east-1".to_string(),
            bucket: "examplebucket".to_string(),
            access_key: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            path_style: false,
        });
        let empty_sha = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let canonical = format!(
            "GET\n/test.txt\n\nhost:examplebucket.s3.amazonaws.com\nrange:bytes=0-9\nx-amz-content-sha256:{empty_sha}\nx-amz-date:20130524T000000Z\n\nhost;range;x-amz-content-sha256;x-amz-date\n{empty_sha}"
        );
        let signature = svc.signature(&canonical, "20130524T000000Z", "20130524");
        assert_eq!(signature.len(), 64, "signature must be 64 hex chars");
        assert_eq!(
            signature,
            "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41",
            "documented AWS example signature must be reproduced",
        );
    }

    #[test]
    fn config_from_env_requires_bucket_and_keys() {
        unsafe {
            std::env::remove_var("PARROT_S3_BUCKET");
            std::env::remove_var("PARROT_S3_ACCESS_KEY");
            std::env::remove_var("PARROT_S3_SECRET_KEY");
        }
        assert!(S3Config::from_env().is_err(), "missing config must error");
        unsafe {
            std::env::set_var("PARROT_S3_BUCKET", "b");
            std::env::set_var("PARROT_S3_ACCESS_KEY", "k");
            std::env::set_var("PARROT_S3_SECRET_KEY", "s");
        }
        let config = S3Config::from_env().expect("configured env must parse");
        assert_eq!(config.bucket, "b");
        assert_eq!(config.region, "us-east-1");
        unsafe {
            std::env::remove_var("PARROT_S3_BUCKET");
            std::env::remove_var("PARROT_S3_ACCESS_KEY");
            std::env::remove_var("PARROT_S3_SECRET_KEY");
        }
    }

    #[test]
    fn presigned_url_contains_sigv4_params() {
        let svc = S3StorageService::new(S3Config {
            endpoint: "https://s3.amazonaws.com".to_string(),
            region: "us-east-1".to_string(),
            bucket: "parrot-assets".to_string(),
            access_key: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            path_style: false,
        });
        let url = svc.presigned_get_url("c1/ns/file.txt", 300);
        assert!(url.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"), "algorithm param");
        assert!(url.contains("X-Amz-Signature="), "signature param");
        assert!(url.contains("X-Amz-Expires=300"), "expiry param");
        assert!(url.contains("X-Amz-SignedHeaders=host"), "signed headers param");
        // Host is bucket.s3.amazonaws.com in virtual-hosted style.
        assert!(url.starts_with("https://parrot-assets.s3.amazonaws.com/c1/ns/file.txt"), "virtual-host URL: {url}");
    }
}
