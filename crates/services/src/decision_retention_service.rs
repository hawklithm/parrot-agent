use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
    // TODO: 添加必要的依赖（database pool, repositories, notification service）
}

impl DefaultDecisionRetentionService {
    pub fn new() -> Self {
        Self {}
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
        _company_id: Uuid,
    ) -> Result<DecisionRetentionState, RetentionError> {
        // TODO: 实现shelving逻辑
        // 1. 检查决策是否存在
        // 2. 更新或创建retention record，设置shelved_at
        // 3. 记录activity log

        let now = Utc::now();
        Ok(DecisionRetentionState {
            id: Uuid::new_v4(),
            company_id: _company_id,
            decision_id,
            shelved_at: Some(now),
            archived_at: None,
            archive_manifest_hash: None,
            created_at: now,
            updated_at: now,
        })
    }

    async fn archive_decision(
        &self,
        decision_id: Uuid,
        _company_id: Uuid,
        manifest: Vec<AttentionArchiveManifestEntry>,
    ) -> Result<DecisionRetentionState, RetentionError> {
        // TODO: 实现archiving逻辑
        // 1. 验证决策已经shelved
        // 2. 计算manifest hash
        // 3. 更新retention record，设置archived_at和manifest_hash
        // 4. 创建归档通知
        // 5. 记录activity log

        let now = Utc::now();
        let manifest_hash = hash_attention_archive_manifest(&manifest);

        Ok(DecisionRetentionState {
            id: Uuid::new_v4(),
            company_id: _company_id,
            decision_id,
            shelved_at: Some(now - Duration::days(DEFAULT_DECISION_SHELF_DAYS)),
            archived_at: Some(now),
            archive_manifest_hash: Some(manifest_hash),
            created_at: now - Duration::days(DEFAULT_DECISION_SHELF_DAYS),
            updated_at: now,
        })
    }

    async fn get_retention_state(
        &self,
        _decision_id: Uuid,
    ) -> Result<Option<DecisionRetentionState>, RetentionError> {
        // TODO: 实现查询逻辑
        Ok(None)
    }

    async fn process_expired_for_shelving(
        &self,
        _company_id: Uuid,
        shelf_days: i64,
    ) -> Result<usize, RetentionError> {
        // TODO: 实现批量shelving逻辑
        // 1. 查找所有超过shelf_days且未shelved的决策
        // 2. 批量更新为shelved状态
        // 3. 返回处理数量

        let _cutoff = self.calculate_shelf_cutoff(shelf_days);
        Ok(0)
    }

    async fn process_expired_for_archiving(
        &self,
        _company_id: Uuid,
        archive_days: i64,
    ) -> Result<usize, RetentionError> {
        // TODO: 实现批量archiving逻辑
        // 1. 查找所有超过archive_days且已shelved但未archived的决策
        // 2. 对每个决策构建attention archive manifest
        // 3. 批量归档
        // 4. 返回处理数量

        let _cutoff = self.calculate_archive_cutoff(archive_days);
        Ok(0)
    }

    async fn send_archive_notifications(
        &self,
    ) -> Result<Vec<ArchiveNotificationBatch>, RetentionError> {
        // TODO: 实现归档通知逻辑
        // 1. 从notification outbox中读取待发送的通知
        // 2. 按agent_id分组
        // 3. 调用notification service发送
        // 4. 标记为已发送

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
}
