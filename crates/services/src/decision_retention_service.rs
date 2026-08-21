use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// 决策保留策略常量
pub const DEFAULT_DECISION_SHELF_DAYS: i64 = 30;
pub const DEFAULT_DECISION_ARCHIVE_DAYS: i64 = 90;

/// 决策保留状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRetentionState {
    pub id: Uuid,
    pub company_id: Uuid,
    pub decision_id: Uuid,
    pub shelved_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
    pub archive_manifest_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 归档通知批次
pub use crate::decision_wakeup_service::{ArchiveNotificationBatch, ArchiveNotificationItem};

/// Attention归档清单条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionArchiveManifestEntry {
    pub source_kind: String,
    pub source_id: String,
    pub issue_id: Option<Uuid>,
    pub archived_at: DateTime<Utc>,
}

/// 决策保留服务
#[async_trait]
pub trait DecisionRetentionService: Send + Sync {
    /// 将决策标记为shelved（搁置）
    async fn shelf_decision(
        &self,
        decision_id: Uuid,
        company_id: Uuid,
    ) -> Result<DecisionRetentionState, RetentionError>;

    /// 归档决策
    async fn archive_decision(
        &self,
        decision_id: Uuid,
        company_id: Uuid,
        manifest: Vec<AttentionArchiveManifestEntry>,
    ) -> Result<DecisionRetentionState, RetentionError>;

    /// 获取决策保留状态
    async fn get_retention_state(
        &self,
        decision_id: Uuid,
    ) -> Result<Option<DecisionRetentionState>, RetentionError>;

    /// 批量处理过期的决策（shelving）
    async fn process_expired_for_shelving(
        &self,
        company_id: Uuid,
        shelf_days: i64,
    ) -> Result<usize, RetentionError>;

    /// 批量处理过期的决策（archiving）
    async fn process_expired_for_archiving(
        &self,
        company_id: Uuid,
        archive_days: i64,
    ) -> Result<usize, RetentionError>;

    /// 发送归档通知
    async fn send_archive_notifications(
        &self,
    ) -> Result<Vec<ArchiveNotificationBatch>, RetentionError>;
}

#[derive(Debug, thiserror::Error)]
pub enum RetentionError {
    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Decision not found: {0}")]
    DecisionNotFound(Uuid),

    #[error("Invalid retention state: {0}")]
    InvalidState(String),

    #[error("Notification error: {0}")]
    NotificationError(String),
}

/// 计算归档清单的SHA256哈希
pub fn hash_attention_archive_manifest(manifest: &[AttentionArchiveManifestEntry]) -> String {
    let canonical = canonical_manifest(manifest);
    let json = serde_json::to_string(&canonical).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// 规范化归档清单（用于一致性哈希）
fn canonical_manifest(
    manifest: &[AttentionArchiveManifestEntry],
) -> Vec<AttentionArchiveManifestEntry> {
    let mut sorted = manifest.to_vec();
    sorted.sort_by(|a, b| {
        let a_key = format!("{}:{}", a.source_kind, a.source_id);
        let b_key = format!("{}:{}", b.source_kind, b.source_id);
        a_key.cmp(&b_key)
    });
    sorted
}

/// 默认决策保留服务实现
pub struct DefaultDecisionRetentionService {
    states: Arc<Mutex<HashMap<Uuid, DecisionRetentionState>>>,
}

impl DefaultDecisionRetentionService {
    pub fn new() -> Self {
        Self {
            states: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 计算决策的shelf截止时间
    pub fn calculate_shelf_cutoff(&self, shelf_days: i64) -> DateTime<Utc> {
        Utc::now() - Duration::days(shelf_days)
    }

    /// 计算决策的archive截止时间
    pub fn calculate_archive_cutoff(&self, archive_days: i64) -> DateTime<Utc> {
        Utc::now() - Duration::days(archive_days)
    }
}

#[async_trait]
impl DecisionRetentionService for DefaultDecisionRetentionService {
    async fn shelf_decision(
        &self,
        decision_id: Uuid,
        company_id: Uuid,
    ) -> Result<DecisionRetentionState, RetentionError> {
        let now = Utc::now();
        let mut states = self.states.lock().await;
        if let Some(state) = states.get_mut(&decision_id) {
            if state.company_id != company_id {
                return Err(RetentionError::DecisionNotFound(decision_id));
            }
            if state.shelved_at.is_none() {
                state.shelved_at = Some(now);
                state.updated_at = now;
            }
            return Ok(state.clone());
        }
        let state = DecisionRetentionState {
            id: Uuid::new_v4(),
            company_id,
            decision_id,
            shelved_at: Some(now),
            archived_at: None,
            archive_manifest_hash: None,
            created_at: now,
            updated_at: now,
        };
        states.insert(decision_id, state.clone());
        Ok(state)
    }

    async fn archive_decision(
        &self,
        decision_id: Uuid,
        company_id: Uuid,
        manifest: Vec<AttentionArchiveManifestEntry>,
    ) -> Result<DecisionRetentionState, RetentionError> {
        let now = Utc::now();
        let manifest_hash = hash_attention_archive_manifest(&manifest);
        let mut states = self.states.lock().await;
        let state = states
            .get_mut(&decision_id)
            .ok_or(RetentionError::DecisionNotFound(decision_id))?;
        if state.company_id != company_id {
            return Err(RetentionError::DecisionNotFound(decision_id));
        }
        if state.shelved_at.is_none() {
            return Err(RetentionError::InvalidState(
                "decision must be shelved before archive".to_string(),
            ));
        }
        if state.archived_at.is_none() {
            state.archived_at = Some(now);
            state.archive_manifest_hash = Some(manifest_hash);
            state.updated_at = now;
        }
        Ok(state.clone())
    }

    async fn get_retention_state(
        &self,
        decision_id: Uuid,
    ) -> Result<Option<DecisionRetentionState>, RetentionError> {
        Ok(self.states.lock().await.get(&decision_id).cloned())
    }

    async fn process_expired_for_shelving(
        &self,
        company_id: Uuid,
        shelf_days: i64,
    ) -> Result<usize, RetentionError> {
        let cutoff = self.calculate_shelf_cutoff(shelf_days);
        let now = Utc::now();
        let mut count = 0;
        for state in self.states.lock().await.values_mut().filter(|state| {
            state.company_id == company_id
                && state.shelved_at.is_none()
                && state.created_at <= cutoff
        }) {
            state.shelved_at = Some(now);
            state.updated_at = now;
            count += 1;
        }
        Ok(count)
    }

    async fn process_expired_for_archiving(
        &self,
        company_id: Uuid,
        archive_days: i64,
    ) -> Result<usize, RetentionError> {
        let cutoff = self.calculate_archive_cutoff(archive_days);
        let now = Utc::now();
        let mut count = 0;
        for state in self.states.lock().await.values_mut().filter(|state| {
            state.company_id == company_id
                && state
                    .shelved_at
                    .is_some_and(|shelved_at| shelved_at <= cutoff)
                && state.archived_at.is_none()
        }) {
            state.archived_at = Some(now);
            state.archive_manifest_hash = Some(hash_attention_archive_manifest(&[]));
            state.updated_at = now;
            count += 1;
        }
        Ok(count)
    }

    async fn send_archive_notifications(
        &self,
    ) -> Result<Vec<ArchiveNotificationBatch>, RetentionError> {
        // The local fallback has no origin-agent/outbox store. Production
        // delivery is provided by PgDecisionRetentionRuntime.
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_attention_archive_manifest() {
        let manifest = vec![AttentionArchiveManifestEntry {
            source_kind: "issue".to_string(),
            source_id: "test-1".to_string(),
            issue_id: Some(Uuid::new_v4()),
            archived_at: Utc::now(),
        }];

        let hash1 = hash_attention_archive_manifest(&manifest);
        let hash2 = hash_attention_archive_manifest(&manifest);

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // SHA256 produces 64 hex chars
    }

    #[test]
    fn test_canonical_manifest_sorting() {
        let now = Utc::now();
        let manifest = vec![
            AttentionArchiveManifestEntry {
                source_kind: "issue".to_string(),
                source_id: "z".to_string(),
                issue_id: None,
                archived_at: now,
            },
            AttentionArchiveManifestEntry {
                source_kind: "issue".to_string(),
                source_id: "a".to_string(),
                issue_id: None,
                archived_at: now,
            },
        ];

        let canonical = canonical_manifest(&manifest);
        assert_eq!(canonical[0].source_id, "a");
        assert_eq!(canonical[1].source_id, "z");
    }

    #[tokio::test]
    async fn test_shelf_archive_are_stateful_and_idempotent() {
        let service = DefaultDecisionRetentionService::new();
        let company_id = Uuid::new_v4();
        let decision_id = Uuid::new_v4();

        assert!(matches!(
            service
                .archive_decision(decision_id, company_id, Vec::new())
                .await,
            Err(RetentionError::DecisionNotFound(_))
        ));
        let shelved = service
            .shelf_decision(decision_id, company_id)
            .await
            .unwrap();
        let shelved_again = service
            .shelf_decision(decision_id, company_id)
            .await
            .unwrap();
        assert_eq!(shelved.id, shelved_again.id);
        assert_eq!(shelved.shelved_at, shelved_again.shelved_at);

        let archived = service
            .archive_decision(decision_id, company_id, Vec::new())
            .await
            .unwrap();
        let archived_again = service
            .archive_decision(decision_id, company_id, Vec::new())
            .await
            .unwrap();
        assert!(archived.archived_at.is_some());
        assert_eq!(
            archived.archive_manifest_hash,
            archived_again.archive_manifest_hash
        );
        assert_eq!(
            service
                .get_retention_state(decision_id)
                .await
                .unwrap()
                .unwrap()
                .id,
            shelved.id
        );
    }

    #[tokio::test]
    async fn test_expired_processing_is_scoped_and_counted() {
        let service = DefaultDecisionRetentionService::new();
        let company_id = Uuid::new_v4();
        let other_company_id = Uuid::new_v4();
        let old_id = Uuid::new_v4();
        let other_id = Uuid::new_v4();
        let now = Utc::now();
        service.states.lock().await.insert(
            old_id,
            DecisionRetentionState {
                id: Uuid::new_v4(),
                company_id,
                decision_id: old_id,
                shelved_at: None,
                archived_at: None,
                archive_manifest_hash: None,
                created_at: now - Duration::days(60),
                updated_at: now,
            },
        );
        service.states.lock().await.insert(
            other_id,
            DecisionRetentionState {
                id: Uuid::new_v4(),
                company_id: other_company_id,
                decision_id: other_id,
                shelved_at: None,
                archived_at: None,
                archive_manifest_hash: None,
                created_at: now - Duration::days(60),
                updated_at: now,
            },
        );

        assert_eq!(
            service
                .process_expired_for_shelving(company_id, 30)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            service
                .process_expired_for_archiving(company_id, -1)
                .await
                .unwrap(),
            1
        );
        assert!(service
            .get_retention_state(old_id)
            .await
            .unwrap()
            .unwrap()
            .archived_at
            .is_some());
        assert!(service
            .get_retention_state(other_id)
            .await
            .unwrap()
            .unwrap()
            .archived_at
            .is_none());
    }
}
