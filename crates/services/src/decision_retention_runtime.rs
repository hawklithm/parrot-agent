use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::decision_retention_service::{RetentionError, DEFAULT_DECISION_ARCHIVE_DAYS};
use crate::decision_wakeup_service::{
    ArchiveNotificationBatch, ArchiveNotificationItem, DecisionWakeupService,
};

/// PostgreSQL-backed retention runtime used by the server scheduler.
pub struct PgDecisionRetentionRuntime {
    pool: PgPool,
    wakeup: Option<Arc<dyn DecisionWakeupService>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotificationDeliveryResult {
    pub notified_agents: usize,
    pub delivered: usize,
}

impl PgDecisionRetentionRuntime {
    pub fn new(pool: PgPool) -> Self {
        Self { pool, wakeup: None }
    }

    pub fn with_wakeup(mut self, wakeup: Arc<dyn DecisionWakeupService>) -> Self {
        self.wakeup = Some(wakeup);
        self
    }

    /// Archive idle, non-kept attention sources and enqueue one notification
    /// per source/archive version. The version predicate makes retries safe.
    pub async fn auto_archive_company(
        &self,
        company_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<usize, RetentionError> {
        let cutoff = now - Duration::days(DEFAULT_DECISION_ARCHIVE_DAYS);
        let mut tx = self.pool.begin().await.map_err(db_retention_error)?;
        let rows = sqlx::query(
            "UPDATE decision_retention
                SET archived_at = $2, archived_reason = 'idle_ttl',
                    archived_by_type = 'system', archived_by_agent_id = NULL,
                    archived_by_user_id = NULL, archived_by_run_id = NULL,
                    archive_version = archive_version + 1, version = version + 1,
                    updated_at = $2
              WHERE company_id = $1 AND keep = FALSE AND archived_at IS NULL
                AND source_activity_at <= $3
              RETURNING source_kind, source_id, archive_version",
        )
        .bind(company_id)
        .bind(now)
        .bind(cutoff)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_retention_error)?;

        for row in &rows {
            let source_kind: String = row.get("source_kind");
            let source_id: String = row.get("source_id");
            let archive_version: i32 = row.get("archive_version");
            if let Some((origin_agent_id, origin_issue_id)) =
                resolve_retention_origin(&self.pool, company_id, &source_kind, &source_id).await?
            {
                sqlx::query(
                    "INSERT INTO decision_archive_notification_outbox
                        (company_id, source_kind, source_id, archive_version,
                         origin_agent_id, origin_issue_id)
                     VALUES ($1, $2, $3, $4, $5, $6)
                     ON CONFLICT (company_id, source_kind, source_id, archive_version)
                     DO NOTHING",
                )
                .bind(company_id)
                .bind(&source_kind)
                .bind(&source_id)
                .bind(archive_version)
                .bind(origin_agent_id)
                .bind(origin_issue_id)
                .execute(&mut *tx)
                .await
                .map_err(db_retention_error)?;
            }
        }
        tx.commit().await.map_err(db_retention_error)?;
        Ok(rows.len())
    }

    /// Claim pending rows with row locks, deliver grouped batches, and return
    /// failed deliveries to pending for a later retry.
    pub async fn deliver_notifications(
        &self,
        limit: i64,
    ) -> Result<NotificationDeliveryResult, RetentionError> {
        let Some(wakeup) = self.wakeup.as_ref() else {
            return Ok(NotificationDeliveryResult {
                notified_agents: 0,
                delivered: 0,
            });
        };
        let limit = limit.clamp(1, 5000);
        let stale_cutoff = Utc::now() - Duration::minutes(5);
        sqlx::query(
            "UPDATE decision_archive_notification_outbox
                SET status = 'pending'
              WHERE status = 'delivering'
                AND (last_attempt_at IS NULL OR last_attempt_at <= $1)",
        )
        .bind(stale_cutoff)
        .execute(&self.pool)
        .await
        .map_err(db_retention_error)?;
        let mut tx = self.pool.begin().await.map_err(db_retention_error)?;
        let rows = sqlx::query(
            "SELECT id, company_id, source_kind, source_id, archive_version,
                    origin_agent_id, origin_issue_id
               FROM decision_archive_notification_outbox
              WHERE status = 'pending'
              ORDER BY created_at ASC, id ASC LIMIT $1
              FOR UPDATE SKIP LOCKED",
        )
        .bind(limit)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_retention_error)?;
        if rows.is_empty() {
            tx.commit().await.map_err(db_retention_error)?;
            return Ok(NotificationDeliveryResult {
                notified_agents: 0,
                delivered: 0,
            });
        }
        let ids: Vec<Uuid> = rows.iter().map(|row| row.get("id")).collect();
        sqlx::query(
            "UPDATE decision_archive_notification_outbox
                SET status = 'delivering', last_attempt_at = NOW(), attempt_count = attempt_count + 1
              WHERE id = ANY($1) AND status = 'pending'",
        )
        .bind(&ids)
        .execute(&mut *tx)
        .await
        .map_err(db_retention_error)?;
        tx.commit().await.map_err(db_retention_error)?;

        let mut groups: HashMap<(Uuid, Uuid), Vec<(Uuid, String, String, i32, Uuid)>> =
            HashMap::new();
        for row in rows {
            let key = (row.get("company_id"), row.get("origin_agent_id"));
            groups.entry(key).or_default().push((
                row.get("id"),
                row.get("source_kind"),
                row.get("source_id"),
                row.get("archive_version"),
                row.get("origin_issue_id"),
            ));
        }

        let mut result = NotificationDeliveryResult {
            notified_agents: 0,
            delivered: 0,
        };
        for ((company_id, agent_id), items) in groups {
            let batch = ArchiveNotificationBatch {
                company_id,
                agent_id,
                items: items
                    .iter()
                    .map(
                        |(_, source_kind, source_id, version, issue_id)| ArchiveNotificationItem {
                            source_kind: source_kind.clone(),
                            source_id: source_id.clone(),
                            issue_id: *issue_id,
                            archive_version: *version,
                        },
                    )
                    .collect(),
            };
            let delivered = wakeup.notify_origin_agent_for_archives(batch).await.is_ok();
            let row_ids: Vec<Uuid> = items.iter().map(|item| item.0).collect();
            sqlx::query(
                "UPDATE decision_archive_notification_outbox
                    SET status = $2,
                        last_attempt_at = NOW(),
                        delivered_at = CASE WHEN $2 = 'delivered' THEN NOW() ELSE delivered_at END
                  WHERE id = ANY($1) AND status = 'delivering'",
            )
            .bind(&row_ids)
            .bind(if delivered { "delivered" } else { "pending" })
            .execute(&self.pool)
            .await
            .map_err(db_retention_error)?;
            if delivered {
                result.notified_agents += 1;
                result.delivered += row_ids.len();
            }
        }
        Ok(result)
    }
}

fn db_retention_error(error: sqlx::Error) -> RetentionError {
    RetentionError::DatabaseError(error.to_string())
}

async fn resolve_retention_origin(
    pool: &PgPool,
    company_id: Uuid,
    source_kind: &str,
    source_id: &str,
) -> Result<Option<(Uuid, Uuid)>, RetentionError> {
    let row = match source_kind {
        "decision" => sqlx::query(
            "SELECT origin_agent_id, origin_issue_id FROM decisions
              WHERE company_id = $1 AND id = $2::uuid",
        )
        .bind(company_id)
        .bind(source_id)
        .fetch_optional(pool)
        .await
        .map_err(db_retention_error)?,
        "approval" => sqlx::query(
            "SELECT a.requested_by_agent_id, ia.issue_id FROM approvals a
              JOIN issue_approvals ia ON ia.approval_id = a.id AND ia.company_id = $1
              WHERE a.company_id = $1 AND a.id = $2::uuid LIMIT 1",
        )
        .bind(company_id)
        .bind(source_id)
        .fetch_optional(pool)
        .await
        .map_err(db_retention_error)?,
        "issue_thread_interaction" => sqlx::query(
            "SELECT created_by_agent_id, issue_id FROM issue_thread_interactions
              WHERE company_id = $1 AND id = $2::uuid",
        )
        .bind(company_id)
        .bind(source_id)
        .fetch_optional(pool)
        .await
        .map_err(db_retention_error)?,
        "productivity_review" | "blocker_attention" | "review" => sqlx::query(
            "SELECT created_by_agent_id, id FROM issues
              WHERE company_id = $1 AND id = $2::uuid",
        )
        .bind(company_id)
        .bind(source_id)
        .fetch_optional(pool)
        .await
        .map_err(db_retention_error)?,
        _ => None,
    };
    let Some(row) = row else {
        return Ok(None);
    };
    let agent_id: Option<Uuid> = row.try_get(0).map_err(db_retention_error)?;
    let issue_id: Option<Uuid> = row.try_get(1).map_err(db_retention_error)?;
    Ok(agent_id.zip(issue_id))
}
