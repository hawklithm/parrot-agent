use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::errors::{ServiceError, ServiceResult};
use models::work_timeline::*;

#[derive(Debug, Clone, Default)]
pub struct WorkTimelineQuery {
    pub company_id: Uuid,
    pub issue_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[async_trait]
pub trait WorkTimelineService: Send + Sync {
    /// Collect issue IDs from multiple sources
    async fn collect_issue_ids(
        &self,
        query: &WorkTimelineQuery,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> ServiceResult<Vec<Uuid>>;

    /// Load heartbeat runs and generate WorkTimelineSpan
    async fn load_heartbeat_runs(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> ServiceResult<Vec<WorkTimelineSpan>>;

    /// Load issue comments and generate WorkTimelineEvent
    async fn load_issue_comments(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> ServiceResult<Vec<WorkTimelineEvent>>;

    /// Load approvals and generate WorkTimelineEvent
    async fn load_approvals(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> ServiceResult<Vec<WorkTimelineEvent>>;

    /// Load issue thread interactions
    async fn load_thread_interactions(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
    ) -> ServiceResult<Vec<serde_json::Value>>;

    /// Extract collaboration edges
    async fn extract_edges(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
    ) -> ServiceResult<Vec<WorkTimelineEdge>>;

    /// Apply user lens filter
    async fn apply_user_lens(
        &self,
        company_id: Uuid,
        user_id: Uuid,
        issue_ids: Vec<Uuid>,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> ServiceResult<Vec<Uuid>>;

    /// Load actors
    async fn load_actors(&self, actor_ids: &[String]) -> ServiceResult<Vec<WorkTimelineActor>>;

    /// Legacy method
    async fn load_events(&self, query: &WorkTimelineQuery) -> ServiceResult<Vec<serde_json::Value>>;
}

pub struct DefaultWorkTimelineService {
    pub pool: PgPool,
}

#[async_trait]
impl WorkTimelineService for DefaultWorkTimelineService {
    async fn collect_issue_ids(
        &self,
        query: &WorkTimelineQuery,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> ServiceResult<Vec<Uuid>> {
        if let Some(issue_id) = query.issue_id {
            return Ok(vec![issue_id]);
        }

        let mut sql = String::from(
            r#"
            WITH candidate_issues AS (
                SELECT DISTINCT resource_id as id
                FROM activity_logs
                WHERE company_id = $1 AND resource_type = 'issue'
                  AND resource_id IS NOT NULL AND created_at BETWEEN $2 AND $3
                UNION
                SELECT DISTINCT (context_snapshot->>'issueId')::uuid as id
                FROM heartbeat_runs
                WHERE company_id = $1 AND context_snapshot->>'issueId' IS NOT NULL
                  AND (started_at BETWEEN $2 AND $3 OR finished_at BETWEEN $2 AND $3
                       OR (started_at < $2 AND (finished_at IS NULL OR finished_at > $2)))
                UNION
                SELECT DISTINCT issue_id as id FROM issue_comments
                WHERE company_id = $1 AND created_at BETWEEN $2 AND $3 AND deleted_at IS NULL
                UNION
                SELECT DISTINCT issue_id as id FROM issue_thread_interactions
                WHERE company_id = $1 AND created_at BETWEEN $2 AND $3
            )
            SELECT DISTINCT i.id FROM candidate_issues ci
            JOIN issues i ON i.id = ci.id WHERE i.company_id = $1
            "#,
        );

        let mut bind_idx = 4;
        if query.project_id.is_some() {
            sql.push_str(&format!(" AND i.project_id = ${}", bind_idx));
            bind_idx += 1;
        }
        if query.goal_id.is_some() {
            sql.push_str(&format!(" AND i.goal_id = ${}", bind_idx));
        }
        sql.push_str(" ORDER BY i.id");

        let mut db_query = sqlx::query(&sql).bind(query.company_id).bind(from).bind(to);
        if let Some(project_id) = query.project_id {
            db_query = db_query.bind(project_id);
        }
        if let Some(goal_id) = query.goal_id {
            db_query = db_query.bind(goal_id);
        }

        let rows = db_query.fetch_all(&self.pool).await
            .map_err(|e| ServiceError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(|r| r.get::<Uuid, _>("id")).collect())
    }

    async fn load_heartbeat_runs(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> ServiceResult<Vec<WorkTimelineSpan>> {
        if issue_ids.is_empty() {
            return Ok(vec![]);
        }

        let rows = sqlx::query(
            r#"
            SELECT hr.id as run_id, hr.agent_id, hr.status::text, hr.started_at, hr.finished_at,
                   hr.invocation_source, hr.context_snapshot, i.id as issue_id,
                   i.identifier as issue_identifier, i.title as issue_title, i.project_id
            FROM heartbeat_runs hr
            LEFT JOIN issues i ON i.execution_run_id = hr.id
            WHERE hr.company_id = $1 AND (hr.context_snapshot->>'issueId')::uuid = ANY($2)
              AND (hr.started_at BETWEEN $3 AND $4 OR hr.finished_at BETWEEN $3 AND $4
                   OR (hr.started_at < $3 AND (hr.finished_at IS NULL OR hr.finished_at > $3)))
            ORDER BY hr.started_at DESC
            "#,
        )
        .bind(company_id).bind(issue_ids).bind(from).bind(to)
        .fetch_all(&self.pool).await
        .map_err(|e| ServiceError::Internal(format!("Failed to load heartbeat runs: {}", e)))?;

        let spans = rows.into_iter().map(|row| {
            let agent_id: Uuid = row.get("agent_id");
            let actor_id = format!("agent:{}", agent_id);
            
            // 尝试从 context_snapshot 中提取 usage
            let context_snapshot: Option<serde_json::Value> = row.get("context_snapshot");
            let usage = context_snapshot
                .and_then(|ctx| ctx.get("usage").cloned())
                .and_then(|u| serde_json::from_value::<RunUsage>(u).ok());
            
            WorkTimelineSpan {
                actor_id,
                lane_hint: None,
                run_id: row.get::<Uuid, _>("id").to_string(),
                issue_id: row.get::<Uuid, _>("issue_id").to_string(),
                issue_identifier: row.get("issue_identifier"),
                issue_title: row.get("issue_title"),
                start: row.get::<Option<DateTime<Utc>>, _>("started_at")
                    .unwrap_or_else(|| Utc::now()).to_rfc3339(),
                end: row.get::<Option<DateTime<Utc>>, _>("finished_at").map(|t| t.to_rfc3339()),
                status: row.get("status"),
                retry_of_run_id: None,
                continuation_attempt: None,
                invocation_source: row.get("invocation_source"),
                usage,
            }
        }).collect();

        Ok(spans)
    }

    async fn load_issue_comments(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> ServiceResult<Vec<WorkTimelineEvent>> {
        if issue_ids.is_empty() {
            return Ok(vec![]);
        }

        let rows = sqlx::query(
            r#"
            SELECT ic.issue_id, ic.author_type::text, ic.author_agent_id, ic.author_user_id, ic.created_at
            FROM issue_comments ic
            WHERE ic.company_id = $1 AND ic.issue_id = ANY($2)
              AND ic.created_at BETWEEN $3 AND $4 AND ic.deleted_at IS NULL
            ORDER BY ic.created_at DESC
            "#,
        )
        .bind(company_id).bind(issue_ids).bind(from).bind(to)
        .fetch_all(&self.pool).await
        .map_err(|e| ServiceError::Internal(format!("Failed to load comments: {}", e)))?;

        let events = rows.into_iter().map(|row| {
            let author_type: String = row.get("author_type");
            let actor_id = if author_type == "agent" {
                format!("agent:{}", row.get::<Option<Uuid>, _>("author_agent_id").unwrap_or_default())
            } else if author_type == "user" {
                format!("user:{}", row.get::<Option<Uuid>, _>("author_user_id").unwrap_or_default())
            } else {
                "system:system".to_string()
            };

            WorkTimelineEvent {
                actor_id,
                kind: TimelineEventKind::Commented,
                issue_id: row.get::<Uuid, _>("issue_id").to_string(),
                at: row.get::<DateTime<Utc>, _>("created_at").to_rfc3339(),
            }
        }).collect();

        Ok(events)
    }

    async fn load_approvals(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> ServiceResult<Vec<WorkTimelineEvent>> {
        if issue_ids.is_empty() {
            return Ok(vec![]);
        }

        let rows = sqlx::query(
            r#"
            SELECT ia.issue_id, a.decided_by_user_id, a.decided_at
            FROM issue_approvals ia
            JOIN approvals a ON a.id = ia.approval_id
            WHERE a.company_id = $1 AND ia.issue_id = ANY($2)
              AND a.decided_at BETWEEN $3 AND $4 AND a.decided_by_user_id IS NOT NULL
            ORDER BY a.decided_at DESC
            "#,
        )
        .bind(company_id).bind(issue_ids).bind(from).bind(to)
        .fetch_all(&self.pool).await
        .map_err(|e| ServiceError::Internal(format!("Failed to load approvals: {}", e)))?;

        let events = rows.into_iter().map(|row| {
            WorkTimelineEvent {
                actor_id: format!("user:{}", row.get::<Uuid, _>("decided_by_user_id")),
                kind: TimelineEventKind::Approved,
                issue_id: row.get::<Uuid, _>("issue_id").to_string(),
                at: row.get::<DateTime<Utc>, _>("decided_at").to_rfc3339(),
            }
        }).collect();

        Ok(events)
    }

    async fn load_thread_interactions(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
    ) -> ServiceResult<Vec<serde_json::Value>> {
        if issue_ids.is_empty() {
            return Ok(vec![]);
        }

        let rows = sqlx::query(
            r#"
            SELECT id, issue_id, kind, status::text, source_run_id, created_at
            FROM issue_thread_interactions
            WHERE company_id = $1 AND issue_id = ANY($2)
            ORDER BY created_at DESC
            "#,
        )
        .bind(company_id).bind(issue_ids)
        .fetch_all(&self.pool).await
        .map_err(|e| ServiceError::Internal(format!("Failed to load thread interactions: {}", e)))?;

        Ok(rows.into_iter().map(|row| {
            serde_json::json!({
                "id": row.get::<Uuid, _>("id"),
                "issueId": row.get::<Uuid, _>("issue_id"),
                "kind": row.get::<String, _>("kind"),
                "status": row.get::<String, _>("status"),
                "sourceRunId": row.get::<Option<Uuid>, _>("source_run_id"),
                "createdAt": row.get::<DateTime<Utc>, _>("created_at"),
            })
        }).collect())
    }

    async fn extract_edges(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
    ) -> ServiceResult<Vec<WorkTimelineEdge>> {
        if issue_ids.is_empty() {
            return Ok(vec![]);
        }

        let rows = sqlx::query(
            r#"
            SELECT i.id as issue_id, i.created_by_agent_id, i.assignee_agent_id,
                   i.assignee_user_id, i.created_at,
                   p.created_by_agent_id as parent_created_by_agent_id
            FROM issues i
            LEFT JOIN issues p ON p.id = i.parent_id
            WHERE i.company_id = $1 AND i.id = ANY($2) AND i.parent_id IS NOT NULL
            "#,
        )
        .bind(company_id).bind(issue_ids)
        .fetch_all(&self.pool).await
        .map_err(|e| ServiceError::Internal(format!("Failed to extract edges: {}", e)))?;

        let mut edges = Vec::new();
        for row in rows {
            let issue_id: Uuid = row.get("issue_id");
            let created_at: DateTime<Utc> = row.get("created_at");
            
            if let (Some(from_agent), Some(to_agent)) = (
                row.get::<Option<Uuid>, _>("parent_created_by_agent_id"),
                row.get::<Option<Uuid>, _>("assignee_agent_id"),
            ) {
                edges.push(WorkTimelineEdge {
                    from_actor_id: format!("agent:{}", from_agent),
                    to_actor_id: format!("agent:{}", to_agent),
                    issue_id: issue_id.to_string(),
                    at: created_at.to_rfc3339(),
                    kind: TimelineEdgeKind::Delegation,
                });
            }

            if let (Some(from_agent), Some(to_user)) = (
                row.get::<Option<Uuid>, _>("created_by_agent_id"),
                row.get::<Option<Uuid>, _>("assignee_user_id"),
            ) {
                edges.push(WorkTimelineEdge {
                    from_actor_id: format!("agent:{}", from_agent),
                    to_actor_id: format!("user:{}", to_user),
                    issue_id: issue_id.to_string(),
                    at: created_at.to_rfc3339(),
                    kind: TimelineEdgeKind::Assignment,
                });
            }
        }

        Ok(edges)
    }

    async fn apply_user_lens(
        &self,
        company_id: Uuid,
        user_id: Uuid,
        issue_ids: Vec<Uuid>,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> ServiceResult<Vec<Uuid>> {
        let rows = sqlx::query(
            r#"
            WITH RECURSIVE user_issues AS (
                SELECT DISTINCT i.id FROM issues i
                WHERE i.company_id = $1 AND i.id = ANY($2)
                  AND (i.created_by_user_id = $3 OR i.assignee_user_id = $3
                      OR EXISTS (SELECT 1 FROM issue_comments ic
                                 WHERE ic.issue_id = i.id AND ic.deleted_at IS NULL AND ic.author_user_id = $3
                                   AND ic.created_at BETWEEN $4 AND $5)
                      OR EXISTS (SELECT 1 FROM issue_approvals ia
                                 JOIN approvals a ON a.id = ia.approval_id
                                 WHERE ia.issue_id = i.id AND a.decided_by_user_id = $3
                                   AND a.decided_at BETWEEN $4 AND $5))
                UNION
                SELECT i.id FROM issues i
                INNER JOIN user_issues ui ON i.parent_id = ui.id
                WHERE i.company_id = $1
            )
            SELECT DISTINCT id FROM user_issues
            "#,
        )
        .bind(company_id).bind(&issue_ids).bind(user_id).bind(from).bind(to)
        .fetch_all(&self.pool).await
        .map_err(|e| ServiceError::Internal(format!("Failed to apply user lens: {}", e)))?;

        Ok(rows.into_iter().map(|r| r.get::<Uuid, _>("id")).collect())
    }

    async fn load_actors(&self, actor_ids: &[String]) -> ServiceResult<Vec<WorkTimelineActor>> {
        if actor_ids.is_empty() {
            return Ok(vec![]);
        }

        let mut actors = Vec::new();
        let mut agent_ids = Vec::new();
        let mut user_ids = Vec::new();
        let mut has_system = false;

        for actor_id in actor_ids {
            if let Some(id) = actor_id.strip_prefix("agent:") {
                if let Ok(uuid) = Uuid::parse_str(id) {
                    agent_ids.push(uuid);
                }
            } else if let Some(id) = actor_id.strip_prefix("user:") {
                if let Ok(uuid) = Uuid::parse_str(id) {
                    user_ids.push(uuid);
                }
            } else if actor_id.starts_with("system:") {
                has_system = true;
            }
        }

        if !agent_ids.is_empty() {
            let rows = sqlx::query("SELECT id, name, avatar FROM agents WHERE id = ANY($1)")
                .bind(&agent_ids).fetch_all(&self.pool).await
                .map_err(|e| ServiceError::Internal(format!("Failed to load agents: {}", e)))?;

            for row in rows {
                actors.push(WorkTimelineActor {
                    id: format!("agent:{}", row.get::<Uuid, _>("id")),
                    actor_type: TimelineActorType::Agent,
                    name: row.get("name"),
                    avatar: row.get("avatar"),
                });
            }
        }

        if !user_ids.is_empty() {
            let rows = sqlx::query("SELECT id, name, avatar FROM users WHERE id = ANY($1)")
                .bind(&user_ids).fetch_all(&self.pool).await
                .map_err(|e| ServiceError::Internal(format!("Failed to load users: {}", e)))?;

            for row in rows {
                actors.push(WorkTimelineActor {
                    id: format!("user:{}", row.get::<Uuid, _>("id")),
                    actor_type: TimelineActorType::User,
                    name: row.get("name"),
                    avatar: row.get("avatar"),
                });
            }
        }

        if has_system {
            actors.push(WorkTimelineActor {
                id: "system:system".to_string(),
                actor_type: TimelineActorType::System,
                name: "System".to_string(),
                avatar: None,
            });
        }

        Ok(actors)
    }

    async fn load_events(&self, query: &WorkTimelineQuery) -> ServiceResult<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            r#"
            SELECT id, event_type, actor_id, resource_type, resource_id, metadata, created_at
            FROM activity_logs
            WHERE company_id = $1
              AND ($2::uuid IS NULL OR resource_id = $2)
              AND ($3::uuid IS NULL OR actor_id = $3)
            ORDER BY created_at DESC LIMIT 500
            "#,
        )
        .bind(query.company_id).bind(query.issue_id).bind(query.user_id)
        .fetch_all(&self.pool).await?;

        Ok(rows.into_iter().map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "eventType": r.get::<String, _>("event_type"),
                "actorId": r.get::<Uuid, _>("actor_id"),
                "resourceType": r.get::<String, _>("resource_type"),
                "resourceId": r.get::<Option<Uuid>, _>("resource_id"),
                "metadata": r.get::<serde_json::Value, _>("metadata"),
                "createdAt": r.get::<DateTime<Utc>, _>("created_at")
            })
        }).collect())
    }
}
