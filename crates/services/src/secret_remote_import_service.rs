use async_trait::async_trait;
use models::{
    RemoteSecretImportPreviewRequest, RemoteSecretImportPreviewResult, RemoteSecretImportRequest,
    RemoteSecretImportResult,
};
use uuid::Uuid;
use sqlx::{PgPool, Row};
use sha2::{Digest, Sha256};
use tokio::process::Command;

use crate::errors::ServiceResult;

/// Service for remote secret import (batch import from external providers)
#[async_trait]
pub trait SecretRemoteImportService: Send + Sync {
    /// Preview secrets from external provider (scan and detect conflicts)
    async fn preview(
        &self,
        company_id: Uuid,
        request: RemoteSecretImportPreviewRequest,
    ) -> ServiceResult<RemoteSecretImportPreviewResult>;

    /// Execute batch import (create secrets from external provider)
    async fn execute(
        &self,
        company_id: Uuid,
        request: RemoteSecretImportRequest,
    ) -> ServiceResult<RemoteSecretImportResult>;
}

/// Provider-backed implementation. AWS and GCP use their standard local
/// CLIs (which inherit the user's configured credentials); Vault uses its HTTP
/// API. No candidate is fabricated when a provider is unavailable.
pub struct ProviderSecretRemoteImportService { pool: PgPool }
impl ProviderSecretRemoteImportService { pub fn new(pool: PgPool) -> Self { Self { pool } } }

impl ProviderSecretRemoteImportService {
    async fn config(&self, company_id: Uuid, config_id: Uuid) -> ServiceResult<(String, serde_json::Value)> {
        let row = sqlx::query("SELECT provider, config, status FROM company_secret_provider_configs WHERE id=$1 AND company_id=$2").bind(config_id).bind(company_id).fetch_optional(&self.pool).await?.ok_or_else(|| crate::errors::ServiceError::NotFound("secret provider config not found".into()))?;
        if row.get::<String,_>("status") != "ready" { return Err(crate::errors::ServiceError::InvalidState("secret provider config is not ready".into())); }
        Ok((row.get("provider"), row.get("config")))
    }
    async fn command_json(command: &str, args: &[String]) -> ServiceResult<serde_json::Value> {
        let output = Command::new(command).args(args).output().await.map_err(|e| crate::errors::ServiceError::Internal(format!("{command} is unavailable: {e}")))?;
        if !output.status.success() { return Err(crate::errors::ServiceError::Internal(String::from_utf8_lossy(&output.stderr).trim().to_string())); }
        serde_json::from_slice(&output.stdout).map_err(|e| crate::errors::ServiceError::Internal(format!("invalid {command} response: {e}")))
    }
    async fn list(&self, provider: &str, config: &serde_json::Value, limit: usize) -> ServiceResult<Vec<(String,String)>> {
        match provider {
            "aws_secrets_manager" => {
                let mut args = vec!["secretsmanager".into(), "list-secrets".into(), "--output".into(), "json".into()];
                if let Some(region) = config.get("region").and_then(|v| v.as_str()) { args.extend(["--region".into(), region.into()]); }
                let value = Self::command_json("aws", &args).await?;
                Ok(value.get("SecretList").and_then(|v| v.as_array()).unwrap_or(&vec![]).iter().take(limit).filter_map(|v| Some((v.get("Name")?.as_str()?.to_string(), v.get("ARN").and_then(|x| x.as_str()).unwrap_or_default().to_string()))).collect())
            }
            "gcp_secret_manager" => {
                let mut args = vec!["secrets".into(), "list".into(), "--format=json".into()];
                if let Some(project) = config.get("projectId").or_else(|| config.get("project_id")).and_then(|v| v.as_str()) { args.extend(["--project".into(), project.into()]); }
                let value = Self::command_json("gcloud", &args).await?;
                Ok(value.as_array().unwrap_or(&vec![]).iter().take(limit).filter_map(|v| { let name=v.get("name")?.as_str()?.to_string(); Some((name.rsplit('/').next()?.to_string(), name)) }).collect())
            }
            "vault" => {
                let address=config.get("address").and_then(|v|v.as_str()).ok_or_else(|| crate::errors::ServiceError::Validation("vault address is required".into()))?;
                let mount=config.get("mountPath").or_else(||config.get("mount_path")).and_then(|v|v.as_str()).unwrap_or("secret");
                let token=config.get("token").and_then(|v|v.as_str()).ok_or_else(|| crate::errors::ServiceError::Validation("vault token is required".into()))?;
                let url=format!("{}/v1/{}/metadata?list=true", address.trim_end_matches('/'), mount.trim_matches('/'));
                let value: serde_json::Value=reqwest::Client::new().get(url).header("X-Vault-Token",token).send().await.map_err(|e|crate::errors::ServiceError::Internal(e.to_string()))?.error_for_status().map_err(|e|crate::errors::ServiceError::Internal(e.to_string()))?.json().await.map_err(|e|crate::errors::ServiceError::Internal(e.to_string()))?;
                Ok(value.get("data").and_then(|v|v.get("keys")).and_then(|v|v.as_array()).unwrap_or(&vec![]).iter().take(limit).filter_map(|v|v.as_str().map(|s|(s.to_string(),format!("vault://{mount}/{s}")))).collect())
            }
            other => Err(crate::errors::ServiceError::InvalidInput(format!("provider {other} does not support remote import"))),
        }
    }
    async fn value(&self, provider: &str, config: &serde_json::Value, name: &str, external_ref: &str) -> ServiceResult<String> {
        match provider {
            "aws_secrets_manager" => { let mut args=vec!["secretsmanager".into(),"get-secret-value".into(),"--secret-id".into(),external_ref.into(),"--output".into(),"json".into()]; if let Some(r)=config.get("region").and_then(|v|v.as_str()){args.extend(["--region".into(),r.into()]);} let v=Self::command_json("aws",&args).await?; Ok(v.get("SecretString").and_then(|x|x.as_str()).or_else(||v.get("SecretBinary").and_then(|x|x.as_str())).unwrap_or_default().to_string()) }
            "gcp_secret_manager" => { let mut args=vec!["secrets".into(),"versions".into(),"access".into(),"latest".into(),"--secret".into(),name.into(),"--format=json".into()]; if let Some(p)=config.get("projectId").or_else(||config.get("project_id")).and_then(|v|v.as_str()){args.extend(["--project".into(),p.into()]);} let v=Self::command_json("gcloud",&args).await?; Ok(v.get("payload").and_then(|x|x.get("data")).and_then(|x|x.as_str()).unwrap_or_default().to_string()) }
            "vault" => { let address=config.get("address").and_then(|v|v.as_str()).ok_or_else(||crate::errors::ServiceError::Validation("vault address is required".into()))?; let token=config.get("token").and_then(|v|v.as_str()).ok_or_else(||crate::errors::ServiceError::Validation("vault token is required".into()))?; let path=external_ref.strip_prefix("vault://").unwrap_or(external_ref); let v = reqwest::Client::new().get(format!("{}/v1/{}",address.trim_end_matches('/'),path)).header("X-Vault-Token",token).send().await.map_err(|e|crate::errors::ServiceError::Internal(e.to_string()))?.error_for_status().map_err(|e|crate::errors::ServiceError::Internal(e.to_string()))?.json::<serde_json::Value>().await.map_err(|e|crate::errors::ServiceError::Internal(e.to_string()))?; serde_json::to_string(v.get("data").unwrap_or(&serde_json::Value::Null)).map_err(|e|crate::errors::ServiceError::Internal(e.to_string())) }
            _ => Err(crate::errors::ServiceError::InvalidInput("unsupported provider".into()))
        }
    }
    fn key(name: &str) -> String { name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' }).collect() }
}

#[async_trait]
impl SecretRemoteImportService for ProviderSecretRemoteImportService {
    async fn preview(&self, company_id: Uuid, request: RemoteSecretImportPreviewRequest) -> ServiceResult<RemoteSecretImportPreviewResult> {
        let (provider, config)=self.config(company_id,request.provider_config_id).await?;
        let listed=self.list(&provider,&config,request.max_results.min(500)).await?;
        let mut candidates=Vec::with_capacity(listed.len());
        for (name,external_ref) in listed { let existing:Option<Uuid>=sqlx::query_scalar("SELECT id FROM company_secrets WHERE company_id=$1 AND (name=$2 OR key=$3 OR external_ref=$4) AND deleted_at IS NULL LIMIT 1").bind(company_id).bind(&name).bind(Self::key(&name)).bind(&external_ref).fetch_optional(&self.pool).await?; let status=if existing.is_some(){models::RemoteSecretImportCandidateStatus::Duplicate}else{models::RemoteSecretImportCandidateStatus::Ready}; candidates.push(models::RemoteSecretImportCandidate{name,external_ref,status,existing_secret_id:existing,conflicts:vec![]}); }
        Ok(RemoteSecretImportPreviewResult{provider_config_id:request.provider_config_id,provider,next_token:None,candidates})
    }
    async fn execute(&self, company_id: Uuid, request: RemoteSecretImportRequest) -> ServiceResult<RemoteSecretImportResult> {
        let (provider,config)=self.config(company_id,request.provider_config_id).await?; let listed=self.list(&provider,&config,1000).await?; let map=listed.into_iter().collect::<std::collections::HashMap<_,_>>(); let mut results=vec![];
        for name in request.secret_names { let external_ref=map.get(&name).cloned().unwrap_or_else(||name.clone()); let key=Self::key(&name); let existing:Option<Uuid>=sqlx::query_scalar("SELECT id FROM company_secrets WHERE company_id=$1 AND (name=$2 OR key=$3 OR external_ref=$4) AND deleted_at IS NULL LIMIT 1").bind(company_id).bind(&name).bind(&key).bind(&external_ref).fetch_optional(&self.pool).await?; if existing.is_some() && !request.overwrite_conflicts { results.push(models::RemoteSecretImportRowResult{name,external_ref,status:models::RemoteSecretImportRowStatus::Skipped,secret_id:None,error:Some("secret already exists".into()),conflicts:vec![]}); continue; } let value=match self.value(&provider,&config,&name,&external_ref).await {Ok(v)=>v,Err(e)=>{results.push(models::RemoteSecretImportRowResult{name,external_ref,status:models::RemoteSecretImportRowStatus::Error,secret_id:None,error:Some(e.to_string()),conflicts:vec![]});continue;}}; let mut h=Sha256::new();h.update(value.as_bytes());let sha=format!("{:x}",h.finalize()); let secret_id=if let Some(id)=existing { sqlx::query("UPDATE company_secrets SET provider=$2,provider_config_id=$3,external_ref=$4,managed_mode='external_reference',latest_version=latest_version+1,updated_at=NOW() WHERE id=$1").bind(id).bind(&provider).bind(request.provider_config_id).bind(&external_ref).execute(&self.pool).await?; id } else { sqlx::query_scalar("INSERT INTO company_secrets (company_id,key,name,provider,provider_config_id,managed_mode,external_ref,latest_version) VALUES ($1,$2,$3,$4,$5,'external_reference',$6,1) RETURNING id").bind(company_id).bind(&key).bind(&name).bind(&provider).bind(request.provider_config_id).bind(&external_ref).fetch_one(&self.pool).await? }; let version:i32=sqlx::query_scalar("SELECT latest_version FROM company_secrets WHERE id=$1").bind(secret_id).fetch_one(&self.pool).await?; sqlx::query("INSERT INTO company_secret_versions (secret_id,version,material,value_sha256,provider_version_ref,status,fingerprint_sha256) VALUES ($1,$2,$3,$4,$5,'current',$4)").bind(secret_id).bind(version).bind(serde_json::json!({"value":value})).bind(&sha).bind(&external_ref).execute(&self.pool).await?; results.push(models::RemoteSecretImportRowResult{name,external_ref,status:models::RemoteSecretImportRowStatus::Imported,secret_id:Some(secret_id),error:None,conflicts:vec![]}); }
        let imported=results.iter().filter(|r|r.status==models::RemoteSecretImportRowStatus::Imported).count();let skipped=results.iter().filter(|r|r.status==models::RemoteSecretImportRowStatus::Skipped).count();let errors=results.len()-imported-skipped;Ok(RemoteSecretImportResult{provider_config_id:request.provider_config_id,provider,imported_count:imported,skipped_count:skipped,error_count:errors,results})
    }
}

/// Mock implementation for testing
pub struct MockSecretRemoteImportService;

#[async_trait]
impl SecretRemoteImportService for MockSecretRemoteImportService {
    async fn preview(
        &self,
        _company_id: Uuid,
        request: RemoteSecretImportPreviewRequest,
    ) -> ServiceResult<RemoteSecretImportPreviewResult> {
        use models::{
            RemoteSecretImportCandidate, RemoteSecretImportCandidateStatus,
            RemoteSecretImportConflict,
        };

        Ok(RemoteSecretImportPreviewResult {
            provider_config_id: request.provider_config_id,
            provider: "aws_secrets_manager".to_string(),
            next_token: None,
            candidates: vec![
                RemoteSecretImportCandidate {
                    name: "DATABASE_URL".to_string(),
                    external_ref: "arn:aws:secretsmanager:us-east-1:123456789012:secret:prod/db-AbCdEf".to_string(),
                    status: RemoteSecretImportCandidateStatus::Ready,
                    existing_secret_id: None,
                    conflicts: vec![],
                },
                RemoteSecretImportCandidate {
                    name: "API_KEY".to_string(),
                    external_ref: "arn:aws:secretsmanager:us-east-1:123456789012:secret:prod/api-XyZ123".to_string(),
                    status: RemoteSecretImportCandidateStatus::Duplicate,
                    existing_secret_id: Some(Uuid::new_v4()),
                    conflicts: vec![],
                },
                RemoteSecretImportCandidate {
                    name: "JWT_SECRET".to_string(),
                    external_ref: "arn:aws:secretsmanager:us-east-1:123456789012:secret:prod/jwt-GhI456".to_string(),
                    status: RemoteSecretImportCandidateStatus::Conflict,
                    existing_secret_id: Some(Uuid::new_v4()),
                    conflicts: vec![RemoteSecretImportConflict {
                        field: "provider".to_string(),
                        remote_value: "aws_secrets_manager".to_string(),
                        local_value: "local_encrypted".to_string(),
                    }],
                },
            ],
        })
    }

    async fn execute(
        &self,
        _company_id: Uuid,
        request: RemoteSecretImportRequest,
    ) -> ServiceResult<RemoteSecretImportResult> {
        use models::{RemoteSecretImportRowResult, RemoteSecretImportRowStatus};

        let mut results = Vec::new();
        let mut imported = 0;
        let mut skipped = 0;

        for (i, name) in request.secret_names.iter().enumerate() {
            if i % 3 == 2 {
                // Every third secret already exists
                results.push(RemoteSecretImportRowResult {
                    name: name.clone(),
                    external_ref: format!("arn:aws:secretsmanager:us-east-1:123456789012:secret:{}", name),
                    status: RemoteSecretImportRowStatus::Skipped,
                    secret_id: None,
                    error: Some("Secret already exists".to_string()),
                    conflicts: vec![],
                });
                skipped += 1;
            } else {
                // Import successful
                results.push(RemoteSecretImportRowResult {
                    name: name.clone(),
                    external_ref: format!("arn:aws:secretsmanager:us-east-1:123456789012:secret:{}", name),
                    status: RemoteSecretImportRowStatus::Imported,
                    secret_id: Some(Uuid::new_v4()),
                    error: None,
                    conflicts: vec![],
                });
                imported += 1;
            }
        }

        Ok(RemoteSecretImportResult {
            provider_config_id: request.provider_config_id,
            provider: "aws_secrets_manager".to_string(),
            imported_count: imported,
            skipped_count: skipped,
            error_count: 0,
            results,
        })
    }
}
