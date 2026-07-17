use async_trait::async_trait;
use models::{
    RemoteSecretImportPreviewRequest, RemoteSecretImportPreviewResult, RemoteSecretImportRequest,
    RemoteSecretImportResult,
};
use uuid::Uuid;

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
