use async_trait::async_trait;
use sqlx::PgPool;
use models::{
    Issue, IssueQueryFilter, Pagination, CreateIssueInput, UpdateIssueInput,
    IssueStatus, IssueWorkMode,
};
use uuid::Uuid;
use crate::{issue_repository::IssueRepository, RepositoryError};

pub struct PgIssueRepository {
    pool: PgPool,
}

impl PgIssueRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn load_label_ids(&self, issue_id: Uuid) -> Result<Vec<Uuid>, RepositoryError> {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT label_id FROM issue_labels WHERE issue_id = $1 ORDER BY label_id",
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn load_blocked_by_issue_ids(&self, issue_id: Uuid) -> Result<Vec<Uuid>, RepositoryError> {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT issue_id FROM issue_relations WHERE related_issue_id = $1 AND type = 'blocks' ORDER BY issue_id",
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn load_watchdog(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
    ) -> Result<Option<models::task_watchdog::IssueWatchdog>, RepositoryError> {
        sqlx::query_as::<_, models::task_watchdog::IssueWatchdog>(
            "SELECT * FROM issue_watchdogs WHERE company_id = $1 AND issue_id = $2",
        )
        .bind(company_id)
        .bind(issue_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn attach_labels(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        company_id: Uuid,
        issue_id: Uuid,
        label_ids: &[Uuid],
    ) -> Result<(), RepositoryError> {
        let mut unique_label_ids = label_ids.to_vec();
        unique_label_ids.sort_unstable();
        unique_label_ids.dedup();
        if unique_label_ids.is_empty() {
            return Ok(());
        }

        let found = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM labels WHERE company_id = $1 AND id = ANY($2)",
        )
        .bind(company_id)
        .bind(&unique_label_ids)
        .fetch_one(&mut **tx)
        .await
        .map_err(RepositoryError::DatabaseError)?;
        if found != unique_label_ids.len() as i64 {
            return Err(RepositoryError::InvalidData(
                "one or more labels do not belong to the issue company".to_string(),
            ));
        }

        for label_id in unique_label_ids {
            sqlx::query(
                "INSERT INTO issue_labels (company_id, issue_id, label_id) VALUES ($1, $2, $3)",
            )
            .bind(company_id)
            .bind(issue_id)
            .bind(label_id)
            .execute(&mut **tx)
            .await
            .map_err(RepositoryError::DatabaseError)?;
        }
        Ok(())
    }

    async fn attach_blockers(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        company_id: Uuid,
        issue_id: Uuid,
        blocker_ids: &[Uuid],
        created_by_agent_id: Option<Uuid>,
        created_by_user_id: Option<Uuid>,
    ) -> Result<(), RepositoryError> {
        let mut unique_blocker_ids = blocker_ids.to_vec();
        unique_blocker_ids.sort_unstable();
        unique_blocker_ids.dedup();
        if unique_blocker_ids.is_empty() {
            return Ok(());
        }
        if unique_blocker_ids.contains(&issue_id) {
            return Err(RepositoryError::InvalidData(
                "an issue cannot be blocked by itself".to_string(),
            ));
        }
        let found = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM issues WHERE company_id = $1 AND id = ANY($2)",
        )
        .bind(company_id)
        .bind(&unique_blocker_ids)
        .fetch_one(&mut **tx)
        .await
        .map_err(RepositoryError::DatabaseError)?;
        if found != unique_blocker_ids.len() as i64 {
            return Err(RepositoryError::InvalidData(
                "blocked-by issues must belong to the issue company".to_string(),
            ));
        }
        for blocker_id in unique_blocker_ids {
            let creates_cycle = sqlx::query_scalar::<_, bool>(
                "WITH RECURSIVE reachable(issue_id) AS (
                   SELECT related_issue_id
                     FROM issue_relations
                    WHERE company_id = $1 AND issue_id = $2 AND type = 'blocks'
                   UNION
                   SELECT relation.related_issue_id
                     FROM issue_relations relation
                     JOIN reachable ON reachable.issue_id = relation.issue_id
                    WHERE relation.company_id = $1 AND relation.type = 'blocks'
                 )
                 SELECT EXISTS (SELECT 1 FROM reachable WHERE issue_id = $3)",
            )
            .bind(company_id)
            .bind(issue_id)
            .bind(blocker_id)
            .fetch_one(&mut **tx)
            .await
            .map_err(RepositoryError::DatabaseError)?;
            if creates_cycle {
                return Err(RepositoryError::InvalidData(
                    "blocked-by relation would create a cycle".to_string(),
                ));
            }
            sqlx::query(
                "INSERT INTO issue_relations (company_id, issue_id, related_issue_id, type, created_by_agent_id, created_by_user_id) VALUES ($1, $2, $3, 'blocks', $4, $5)",
            )
            .bind(company_id)
            .bind(blocker_id)
            .bind(issue_id)
            .bind(created_by_agent_id)
            .bind(created_by_user_id)
            .execute(&mut **tx)
            .await
            .map_err(RepositoryError::DatabaseError)?;
        }
        Ok(())
    }

    async fn load_issue_projections(&self, issue: &mut Issue) -> Result<(), RepositoryError> {
        issue.label_ids = self.load_label_ids(issue.id).await?;
        issue.blocked_by_issue_ids = self.load_blocked_by_issue_ids(issue.id).await?;
        issue.watchdog = self.load_watchdog(issue.company_id, issue.id).await?;
        Ok(())
    }

    async fn sync_issue_associations(
        &self,
        issue_id: Uuid,
        label_ids: Option<&[Uuid]>,
        blocked_by_issue_ids: Option<&[Uuid]>,
    ) -> Result<(), RepositoryError> {
        if label_ids.is_none() && blocked_by_issue_ids.is_none() {
            return Ok(());
        }
        let company_id = sqlx::query_scalar::<_, Uuid>(
            "SELECT company_id FROM issues WHERE id = $1",
        )
        .bind(issue_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;
        let mut tx = self.pool.begin().await.map_err(RepositoryError::DatabaseError)?;
        if let Some(label_ids) = label_ids {
            sqlx::query("DELETE FROM issue_labels WHERE company_id = $1 AND issue_id = $2")
                .bind(company_id)
                .bind(issue_id)
                .execute(&mut *tx)
                .await
                .map_err(RepositoryError::DatabaseError)?;
            Self::attach_labels(&mut tx, company_id, issue_id, label_ids).await?;
        }
        if let Some(blocked_by_issue_ids) = blocked_by_issue_ids {
            sqlx::query(
                "DELETE FROM issue_relations WHERE company_id = $1 AND related_issue_id = $2 AND type = 'blocks'",
            )
            .bind(company_id)
            .bind(issue_id)
            .execute(&mut *tx)
            .await
            .map_err(RepositoryError::DatabaseError)?;
            Self::attach_blockers(
                &mut tx,
                company_id,
                issue_id,
                blocked_by_issue_ids,
                None,
                None,
            )
            .await?;
        }
        tx.commit().await.map_err(RepositoryError::DatabaseError)
    }
}

/// Convert an IssueStatus to its database text representation.
/// Uses the Display impl which returns correct snake_case per the sqlx rename_all.
fn issue_status_to_db(s: &IssueStatus) -> String {
    s.to_string()
}

/// Convert an IssueWorkMode to its database text representation (snake_case).
fn issue_work_mode_to_db(wm: &IssueWorkMode) -> String {
    crate::debug_to_snake_case(&format!("{:?}", wm))
}

#[async_trait]
impl IssueRepository for PgIssueRepository {
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Issue>, RepositoryError> {
        let mut issue = sqlx::query_as::<_, Issue>(
            r#"
            SELECT * FROM issues WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        if let Some(issue) = issue.as_mut() {
            self.load_issue_projections(issue).await?;
        }
        Ok(issue)
    }

    async fn list_by_company(
        &self,
        company_id: Uuid,
        filter: &IssueQueryFilter,
        pagination: &Pagination,
    ) -> Result<Vec<Issue>, RepositoryError> {
        let mut query = String::from("SELECT * FROM issues WHERE company_id = $1");
        let mut param_count = 1;

        // Build dynamic query based on filters
        if let Some(statuses) = &filter.status {
            if !statuses.is_empty() {
                param_count += 1;
                query.push_str(&format!(" AND status::text = ANY(${})", param_count));
            }
        }

        if let Some(priorities) = &filter.priority {
            if !priorities.is_empty() {
                param_count += 1;
                query.push_str(&format!(" AND priority = ANY(${})", param_count));
            }
        }

        if let Some(_assignee_agent_id) = filter.assignee_agent_id {
            param_count += 1;
            query.push_str(&format!(" AND assignee_agent_id = ${}", param_count));
        }

        if let Some(_assignee_user_id) = filter.assignee_user_id {
            param_count += 1;
            query.push_str(&format!(" AND assignee_user_id = ${}", param_count));
        }

        if let Some(_project_id) = filter.project_id {
            param_count += 1;
            query.push_str(&format!(" AND project_id = ${}", param_count));
        }

        if let Some(_goal_id) = filter.goal_id {
            param_count += 1;
            query.push_str(&format!(" AND goal_id = ${}", param_count));
        }

        if let Some(_parent_id) = filter.parent_id {
            param_count += 1;
            query.push_str(&format!(" AND parent_id = ${}", param_count));
        }

        if filter.search_query.as_deref().is_some_and(|query| !query.trim().is_empty()) {
            param_count += 1;
            query.push_str(&format!(" AND (title ILIKE '%' || ${} || '%' OR description ILIKE '%' || ${} || '%' OR identifier ILIKE '%' || ${} || '%')", param_count, param_count, param_count));
        }

        if let Some(ref _work_mode) = filter.work_mode {
            param_count += 1;
            query.push_str(&format!(" AND work_mode = ${}", param_count));
        }

        if let Some(_participant_agent_id) = filter.participant_agent_id {
            param_count += 1;
            query.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM issue_comments participant_comments WHERE participant_comments.issue_id = issues.id AND participant_comments.deleted_at IS NULL AND participant_comments.actor_type = 'agent'::comment_actor_type AND participant_comments.actor_id = ${})",
                param_count
            ));
        }
        if let Some(_touched_by_user_id) = filter.touched_by_user_id {
            param_count += 1;
            query.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM issue_comments touched_comments WHERE touched_comments.issue_id = issues.id AND touched_comments.deleted_at IS NULL AND touched_comments.actor_type = 'user'::comment_actor_type AND touched_comments.actor_id = ${})",
                param_count
            ));
        }
        if let Some(_user_id) = filter.inbox_archived_by_user_id {
            param_count += 1;
            query.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM issue_inbox_archives archived_issues WHERE archived_issues.issue_id = issues.id AND archived_issues.company_id = issues.company_id AND archived_issues.user_id = ${})",
                param_count
            ));
        }
        if let Some(_user_id) = filter.unread_for_user_id {
            param_count += 1;
            query.push_str(&format!(
                " AND NOT EXISTS (SELECT 1 FROM issue_read_status read_issues WHERE read_issues.issue_id = issues.id AND read_issues.company_id = issues.company_id AND read_issues.user_id = ${})",
                param_count
            ));
        }
        if let Some(_label_id) = filter.label_id {
            param_count += 1;
            query.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM issue_labels issue_filter_labels WHERE issue_filter_labels.issue_id = issues.id AND issue_filter_labels.label_id = ${})",
                param_count
            ));
        }
        if let Some(_workspace_id) = filter.execution_workspace_id {
            param_count += 1;
            query.push_str(&format!(" AND execution_workspace_id = ${}", param_count));
        }
        if let Some(_origin_kind) = filter.origin_kind.as_deref().filter(|value| !value.is_empty()) {
            param_count += 1;
            query.push_str(&format!(" AND origin_kind = ${}", param_count));
        }
        if let Some(_origin_id) = filter.origin_id.as_deref().filter(|value| !value.is_empty()) {
            param_count += 1;
            query.push_str(&format!(" AND origin_id = ${}", param_count));
        }

        // Add ordering and pagination
        query.push_str(" ORDER BY updated_at DESC");
        param_count += 1;
        query.push_str(&format!(" LIMIT ${}", param_count));
        param_count += 1;
        query.push_str(&format!(" OFFSET ${}", param_count));

        // Build query with all parameters
        let mut q = sqlx::query_as::<_, Issue>(&query).bind(company_id);

        if let Some(statuses) = &filter.status {
            if !statuses.is_empty() {
                let status_strs: Vec<String> = statuses.iter().map(|s| issue_status_to_db(s)).collect();
                q = q.bind(status_strs);
            }
        }

        if let Some(priorities) = &filter.priority {
            if !priorities.is_empty() {
                // IssuePriority uses rename_all = "lowercase", so Debug → lowercase is correct
                let priority_strs: Vec<String> = priorities.iter().map(|p| format!("{:?}", p).to_lowercase()).collect();
                q = q.bind(priority_strs);
            }
        }

        if let Some(assignee_agent_id) = filter.assignee_agent_id {
            q = q.bind(assignee_agent_id);
        }

        if let Some(assignee_user_id) = filter.assignee_user_id {
            q = q.bind(assignee_user_id);
        }

        if let Some(project_id) = filter.project_id {
            q = q.bind(project_id);
        }

        if let Some(goal_id) = filter.goal_id {
            q = q.bind(goal_id);
        }

        if let Some(parent_id) = filter.parent_id {
            q = q.bind(parent_id);
        }

        if let Some(search_query) = filter.search_query.as_deref().filter(|query| !query.trim().is_empty()) {
            q = q.bind(search_query.trim());
        }

        if let Some(ref work_mode) = filter.work_mode {
            let mode_str = issue_work_mode_to_db(work_mode);
            q = q.bind(mode_str);
        }

        if let Some(participant_agent_id) = filter.participant_agent_id {
            q = q.bind(participant_agent_id);
        }
        if let Some(user_id) = filter.touched_by_user_id {
            q = q.bind(user_id);
        }
        if let Some(user_id) = filter.inbox_archived_by_user_id {
            q = q.bind(user_id);
        }
        if let Some(user_id) = filter.unread_for_user_id {
            q = q.bind(user_id);
        }
        if let Some(label_id) = filter.label_id {
            q = q.bind(label_id);
        }
        if let Some(workspace_id) = filter.execution_workspace_id {
            q = q.bind(workspace_id);
        }
        if let Some(origin_kind) = filter.origin_kind.as_deref().filter(|value| !value.is_empty()) {
            q = q.bind(origin_kind);
        }
        if let Some(origin_id) = filter.origin_id.as_deref().filter(|value| !value.is_empty()) {
            q = q.bind(origin_id);
        }

        q = q.bind(pagination.limit).bind(pagination.offset);

        let mut issues = q.fetch_all(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?;

        for issue in &mut issues {
            self.load_issue_projections(issue).await?;
        }

        Ok(issues)
    }

    async fn list_locked_by_company(
        &self,
        company_id: Uuid,
        pagination: &Pagination,
    ) -> Result<Vec<Issue>, RepositoryError> {
        let mut issues = sqlx::query_as::<_, Issue>(
            "SELECT * FROM issues
              WHERE company_id = $1 AND execution_locked_at IS NOT NULL
              ORDER BY execution_locked_at ASC
              LIMIT $2 OFFSET $3",
        )
        .bind(company_id)
        .bind(pagination.limit)
        .bind(pagination.offset)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        for issue in &mut issues {
            self.load_issue_projections(issue).await?;
        }
        Ok(issues)
    }

    async fn count_by_company(
        &self,
        company_id: Uuid,
        filter: &IssueQueryFilter,
    ) -> Result<i64, RepositoryError> {
        let mut query = String::from("SELECT COUNT(*) as count FROM issues WHERE company_id = $1");
        let mut param_count = 1;

        // Build dynamic query based on filters (same logic as list_by_company)
        if let Some(statuses) = &filter.status {
            if !statuses.is_empty() {
                param_count += 1;
                query.push_str(&format!(" AND status::text = ANY(${})", param_count));
            }
        }

        if let Some(priorities) = &filter.priority {
            if !priorities.is_empty() {
                param_count += 1;
                query.push_str(&format!(" AND priority = ANY(${})", param_count));
            }
        }

        if let Some(_assignee_agent_id) = filter.assignee_agent_id {
            param_count += 1;
            query.push_str(&format!(" AND assignee_agent_id = ${}", param_count));
        }

        if let Some(_assignee_user_id) = filter.assignee_user_id {
            param_count += 1;
            query.push_str(&format!(" AND assignee_user_id = ${}", param_count));
        }

        if let Some(_project_id) = filter.project_id {
            param_count += 1;
            query.push_str(&format!(" AND project_id = ${}", param_count));
        }

        if let Some(_goal_id) = filter.goal_id {
            param_count += 1;
            query.push_str(&format!(" AND goal_id = ${}", param_count));
        }

        if let Some(_parent_id) = filter.parent_id {
            param_count += 1;
            query.push_str(&format!(" AND parent_id = ${}", param_count));
        }

        if filter.search_query.as_deref().is_some_and(|query| !query.trim().is_empty()) {
            param_count += 1;
            query.push_str(&format!(" AND (title ILIKE '%' || ${} || '%' OR description ILIKE '%' || ${} || '%' OR identifier ILIKE '%' || ${} || '%')", param_count, param_count, param_count));
        }

        if let Some(ref _work_mode) = filter.work_mode {
            param_count += 1;
            query.push_str(&format!(" AND work_mode = ${}", param_count));
        }

        if let Some(_participant_agent_id) = filter.participant_agent_id {
            param_count += 1;
            query.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM issue_comments participant_comments WHERE participant_comments.issue_id = issues.id AND participant_comments.deleted_at IS NULL AND participant_comments.actor_type = 'agent'::comment_actor_type AND participant_comments.actor_id = ${})",
                param_count
            ));
        }
        if let Some(_touched_by_user_id) = filter.touched_by_user_id {
            param_count += 1;
            query.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM issue_comments touched_comments WHERE touched_comments.issue_id = issues.id AND touched_comments.deleted_at IS NULL AND touched_comments.actor_type = 'user'::comment_actor_type AND touched_comments.actor_id = ${})",
                param_count
            ));
        }
        if let Some(_user_id) = filter.inbox_archived_by_user_id {
            param_count += 1;
            query.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM issue_inbox_archives archived_issues WHERE archived_issues.issue_id = issues.id AND archived_issues.company_id = issues.company_id AND archived_issues.user_id = ${})",
                param_count
            ));
        }
        if let Some(_user_id) = filter.unread_for_user_id {
            param_count += 1;
            query.push_str(&format!(
                " AND NOT EXISTS (SELECT 1 FROM issue_read_status read_issues WHERE read_issues.issue_id = issues.id AND read_issues.company_id = issues.company_id AND read_issues.user_id = ${})",
                param_count
            ));
        }
        if let Some(_label_id) = filter.label_id {
            param_count += 1;
            query.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM issue_labels issue_filter_labels WHERE issue_filter_labels.issue_id = issues.id AND issue_filter_labels.label_id = ${})",
                param_count
            ));
        }
        if let Some(_workspace_id) = filter.execution_workspace_id {
            param_count += 1;
            query.push_str(&format!(" AND execution_workspace_id = ${}", param_count));
        }
        if let Some(_origin_kind) = filter.origin_kind.as_deref().filter(|value| !value.is_empty()) {
            param_count += 1;
            query.push_str(&format!(" AND origin_kind = ${}", param_count));
        }
        if let Some(_origin_id) = filter.origin_id.as_deref().filter(|value| !value.is_empty()) {
            param_count += 1;
            query.push_str(&format!(" AND origin_id = ${}", param_count));
        }

        let mut q = sqlx::query_scalar::<_, i64>(&query).bind(company_id);

        if let Some(statuses) = &filter.status {
            if !statuses.is_empty() {
                let status_strs: Vec<String> = statuses.iter().map(|s| issue_status_to_db(s)).collect();
                q = q.bind(status_strs);
            }
        }

        if let Some(priorities) = &filter.priority {
            if !priorities.is_empty() {
                let priority_strs: Vec<String> = priorities.iter().map(|p| format!("{:?}", p).to_lowercase()).collect();
                q = q.bind(priority_strs);
            }
        }

        if let Some(assignee_agent_id) = filter.assignee_agent_id {
            q = q.bind(assignee_agent_id);
        }

        if let Some(assignee_user_id) = filter.assignee_user_id {
            q = q.bind(assignee_user_id);
        }

        if let Some(project_id) = filter.project_id {
            q = q.bind(project_id);
        }

        if let Some(goal_id) = filter.goal_id {
            q = q.bind(goal_id);
        }

        if let Some(parent_id) = filter.parent_id {
            q = q.bind(parent_id);
        }

        if let Some(search_query) = filter.search_query.as_deref().filter(|query| !query.trim().is_empty()) {
            q = q.bind(search_query.trim());
        }

        if let Some(ref work_mode) = filter.work_mode {
            let mode_str = issue_work_mode_to_db(work_mode);
            q = q.bind(mode_str);
        }

        if let Some(participant_agent_id) = filter.participant_agent_id {
            q = q.bind(participant_agent_id);
        }
        if let Some(user_id) = filter.touched_by_user_id {
            q = q.bind(user_id);
        }
        if let Some(user_id) = filter.inbox_archived_by_user_id {
            q = q.bind(user_id);
        }
        if let Some(user_id) = filter.unread_for_user_id {
            q = q.bind(user_id);
        }
        if let Some(label_id) = filter.label_id {
            q = q.bind(label_id);
        }
        if let Some(workspace_id) = filter.execution_workspace_id {
            q = q.bind(workspace_id);
        }
        if let Some(origin_kind) = filter.origin_kind.as_deref().filter(|value| !value.is_empty()) {
            q = q.bind(origin_kind);
        }
        if let Some(origin_id) = filter.origin_id.as_deref().filter(|value| !value.is_empty()) {
            q = q.bind(origin_id);
        }

        let count = q.fetch_one(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?;

        Ok(count)
    }

    async fn create(&self, input: CreateIssueInput) -> Result<Issue, RepositoryError> {
        let execution_policy = input
            .execution_policy
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|error| RepositoryError::InvalidData(format!("invalid execution policy: {error}")))?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(RepositoryError::DatabaseError)?;

        if let Some(idempotency_key) = input
            .idempotency_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
        {
            let idempotency_guard_key = format!(
                "issue-create:idempotency-tx:{}:{}",
                input.company_id, idempotency_key
            );
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                .bind(&idempotency_guard_key)
                .execute(&mut *tx)
                .await
                .map_err(RepositoryError::DatabaseError)?;
        }
        
        // Generate unique origin_fingerprint to prevent duplicate issue creation
        // Strategy:
        //   - Agent-created issues: hash(run_id + title) - prevents duplicate calls in same run
        //   - Manual issues: timestamp + UUID - allows multiple issues with same title
        let origin_fingerprint = input.origin_fingerprint.clone().unwrap_or_else(|| {
            if let Some(run_id) = input.origin_run_id {
                // For agent-created issues: use run_id + title hash
                // This ensures that if an agent calls create-issue multiple times
                // with the same title in the same run, they get the same fingerprint
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                run_id.hash(&mut hasher);
                input.title.hash(&mut hasher);
                let content_hash = hasher.finish();
                format!("agent:{}:{:x}", run_id, content_hash)
            } else {
                // For manual issues: timestamp + UUID
                // This allows users to create multiple issues with the same title
                let creator = input.created_by_user_id
                    .map(|id| id.to_string())
                    .or_else(|| input.created_by_agent_id.map(|id| id.to_string()))
                    .unwrap_or_else(|| "system".to_string());
                format!("manual:{}:{}:{}", 
                    creator,
                    chrono::Utc::now().timestamp_millis(),
                    Uuid::new_v4()
                )
            }
        });
        
        // Generate identifier for the issue (e.g., "ISSUE-1", "ISSUE-2")
        // Get the next issue number for this company
        let issue_number: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(issue_number), 0) + 1 FROM issues WHERE company_id = $1"
        )
        .bind(input.company_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(RepositoryError::DatabaseError)?;
        
        let identifier = format!("ISSUE-{}", issue_number);
        
        let mut issue = sqlx::query_as::<_, Issue>(
            r#"
            INSERT INTO issues (
                company_id, project_id, project_workspace_id, goal_id, parent_id,
                title, description, status, work_mode, harness_kind, priority,
                assignee_agent_id, assignee_user_id,
                created_by_agent_id, created_by_user_id, responsible_user_id,
                issue_number, identifier,
                origin_kind, origin_id, origin_run_id, origin_fingerprint, request_depth,
                billing_code, assignee_adapter_overrides,
                execution_policy, execution_workspace_settings,
                execution_workspace_id, execution_workspace_preference
            )
            VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9, $10, $11,
                $12, $13,
                $14, $15, $16,
                $17, $18,
                $19, $20, $21, $22, $23,
                $24, $25,
                $26, $27,
                $28, $29
            )
            RETURNING *
            "#,
        )
        .bind(input.company_id)
        .bind(input.project_id)
        .bind(input.project_workspace_id)
        .bind(input.goal_id)
        .bind(input.parent_id)
        .bind(&input.title)
        .bind(input.description.as_ref())
        .bind(input.status)
        .bind(input.work_mode.unwrap_or(models::IssueWorkMode::Standard))
        .bind(input.harness_kind.as_deref())
        .bind(input.priority.unwrap_or(models::IssuePriority::Medium))
        .bind(input.assignee_agent_id)
        .bind(input.assignee_user_id)
        .bind(input.created_by_agent_id)
        .bind(input.created_by_user_id)
        .bind(input.responsible_user_id)
        .bind(issue_number)
        .bind(&identifier)
        // PostgreSQL defaults are not applied when a column is explicitly
        // bound as NULL. Paperclip normalizes ordinary issue creation to the
        // manual origin kind before inserting.
        .bind(input.origin_kind.as_deref().unwrap_or("manual"))
        .bind(input.origin_id.as_ref())
        .bind(input.origin_run_id)
        .bind(&origin_fingerprint)  // Use generated unique fingerprint
        .bind(input.request_depth.unwrap_or(0))
        .bind(input.billing_code.as_ref())
        .bind(&input.assignee_adapter_overrides)
        .bind(&execution_policy)
        .bind(&input.execution_workspace_settings)
        .bind(input.execution_workspace_id)
        .bind(input.execution_workspace_preference.as_ref())
        .fetch_one(&mut *tx)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Self::attach_labels(&mut tx, input.company_id, issue.id, &input.label_ids).await?;
        Self::attach_blockers(
            &mut tx,
            input.company_id,
            issue.id,
            &input.blocked_by_issue_ids,
            input.created_by_agent_id,
            input.created_by_user_id,
        )
        .await?;
        if let Some(watchdog) = input.watchdog.as_ref() {
            let created_by_user_id = input.created_by_user_id.map(|id| id.to_string());
            let watchdog_row = sqlx::query_as::<_, models::task_watchdog::IssueWatchdog>(
                r#"INSERT INTO issue_watchdogs
                   (company_id, issue_id, watchdog_agent_id, instructions,
                    created_by_agent_id, created_by_user_id, created_by_run_id,
                    updated_by_agent_id, updated_by_user_id, updated_by_run_id)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $5, $6, $7)
                   ON CONFLICT (company_id, issue_id) DO UPDATE
                     SET watchdog_agent_id = EXCLUDED.watchdog_agent_id,
                         instructions = EXCLUDED.instructions,
                         updated_by_agent_id = EXCLUDED.updated_by_agent_id,
                         updated_by_user_id = EXCLUDED.updated_by_user_id,
                         updated_by_run_id = EXCLUDED.updated_by_run_id,
                         updated_at = NOW()
                   RETURNING id, company_id, issue_id, watchdog_agent_id, instructions, status,
                             watchdog_issue_id, last_observed_fingerprint, last_reviewed_fingerprint,
                             last_triggered_at, last_completed_at, trigger_count,
                             created_by_agent_id, created_by_user_id, created_by_run_id,
                             updated_by_agent_id, updated_by_user_id, updated_by_run_id,
                             created_at, updated_at"#,
            )
            .bind(input.company_id)
            .bind(issue.id)
            .bind(watchdog.agent_id)
            .bind(watchdog.instructions.as_deref())
            .bind(input.created_by_agent_id)
            .bind(created_by_user_id.as_deref())
            .bind(input.watchdog_created_by_run_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(RepositoryError::DatabaseError)?;
            issue.watchdog = Some(watchdog_row);
        }
        if let Some(audit) = input.watchdog_discovery_audit.as_ref() {
            sqlx::query(
                "INSERT INTO activity_logs
                 (company_id, event_type, actor_type, actor_id, resource_type, resource_id, metadata)
                 VALUES ($1, 'issue.watchdog_discovery_created', 'agent', $2, 'issue', $3, $4)",
            )
            .bind(input.company_id)
            .bind(audit.actor_id)
            .bind(issue.id)
            .bind(serde_json::json!({
                "kind": "product_bug",
                "sourceIssueId": audit.source_issue_id,
                "watchdogIssueId": audit.watchdog_issue_id,
                "watchdogId": audit.watchdog_id,
                "stopFingerprint": audit.stop_fingerprint,
            }))
            .execute(&mut *tx)
            .await
            .map_err(RepositoryError::DatabaseError)?;
        }
        if let Some(idempotency_key) = input
            .idempotency_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
        {
            sqlx::query(
                "INSERT INTO issue_create_idempotency_keys
                    (company_id, idempotency_key, issue_id)
                 VALUES ($1, $2, $3)",
            )
            .bind(input.company_id)
            .bind(idempotency_key)
            .bind(issue.id)
            .execute(&mut *tx)
            .await
            .map_err(RepositoryError::DatabaseError)?;
        }
        issue.label_ids = input.label_ids;
        issue.blocked_by_issue_ids = input.blocked_by_issue_ids;
        tx.commit().await.map_err(RepositoryError::DatabaseError)?;
        Ok(issue)
    }

    async fn update(&self, id: Uuid, input: UpdateIssueInput) -> Result<Issue, RepositoryError> {
        // Build dynamic UPDATE query
        let mut updates = Vec::new();
        let mut param_count = 1;

        if input.title.is_some() {
            param_count += 1;
            updates.push(format!("title = ${}", param_count));
        }
        if input.description.is_some() {
            param_count += 1;
            updates.push(format!("description = ${}", param_count));
        }
        if input.status.is_some() {
            param_count += 1;
            updates.push(format!("status = ${}", param_count));
        }
        if input.priority.is_some() {
            param_count += 1;
            updates.push(format!("priority = ${}", param_count));
        }
        if input.work_mode.is_some() {
            param_count += 1;
            updates.push(format!("work_mode = ${}", param_count));
        }
        if input.harness_kind.is_some() {
            param_count += 1;
            updates.push(format!("harness_kind = ${}", param_count));
        }
        if input.assignee_agent_id.is_some() {
            param_count += 1;
            updates.push(format!("assignee_agent_id = ${}", param_count));
        }
        if input.assignee_user_id.is_some() {
            param_count += 1;
            updates.push(format!("assignee_user_id = ${}", param_count));
        }
        if input.responsible_user_id.is_some() {
            param_count += 1;
            updates.push(format!("responsible_user_id = ${}", param_count));
        }
        if input.execution_policy.is_some() {
            param_count += 1;
            updates.push(format!("execution_policy = ${}", param_count));
        }
        if input.execution_state.is_some() {
            param_count += 1;
            updates.push(format!("execution_state = ${}", param_count));
        }
        if input.monitor_notes.is_some() {
            param_count += 1;
            updates.push(format!("monitor_notes = ${}", param_count));
        }
        if input.monitor_scheduled_by.is_some() {
            param_count += 1;
            updates.push(format!("monitor_scheduled_by = ${}", param_count));
        }
        if input.execution_workspace_preference.is_some() {
            param_count += 1;
            updates.push(format!("execution_workspace_preference = ${}", param_count));
        }
        if input.execution_workspace_settings.is_some() {
            param_count += 1;
            updates.push(format!("execution_workspace_settings = ${}", param_count));
        }
        if input.hidden_at.is_some() {
            param_count += 1;
            updates.push(format!("hidden_at = ${}", param_count));
        }
        if input.source_trust.is_some() {
            param_count += 1;
            updates.push(format!("source_trust = ${}", param_count));
        }

        if updates.is_empty() {
            if input.label_ids.is_some() || input.blocked_by_issue_ids.is_some() {
                let issue = self.get_by_id(id).await?.ok_or(RepositoryError::NotFound(id))?;
                self.sync_issue_associations(
                    id,
                    input.label_ids.as_deref(),
                    input.blocked_by_issue_ids.as_deref(),
                )
                .await?;
                let mut issue = issue;
                self.load_issue_projections(&mut issue).await?;
                return Ok(issue);
            }
            // No fields to update, just return the existing issue
            return self.get_by_id(id).await?.ok_or_else(|| RepositoryError::NotFound(id));
        }

        updates.push("updated_at = NOW()".to_string());

        let query = format!(
            "UPDATE issues SET {} WHERE id = $1 RETURNING *",
            updates.join(", ")
        );

        let mut q = sqlx::query_as::<_, Issue>(&query).bind(id);

        // Bind all parameters in the same order as updates
        if let Some(ref title) = input.title {
            q = q.bind(title);
        }
        if let Some(ref description) = input.description {
            q = q.bind(description);
        }
        if let Some(status) = input.status {
            q = q.bind(status);
        }
        if let Some(priority) = input.priority {
            q = q.bind(priority);
        }
        if let Some(work_mode) = input.work_mode {
            q = q.bind(work_mode);
        }
        if let Some(ref harness_kind) = input.harness_kind {
            q = q.bind(harness_kind);
        }
        if let Some(assignee_agent_id) = input.assignee_agent_id {
            q = q.bind(assignee_agent_id);
        }
        if let Some(assignee_user_id) = input.assignee_user_id {
            q = q.bind(assignee_user_id);
        }
        if let Some(responsible_user_id) = input.responsible_user_id {
            q = q.bind(responsible_user_id);
        }
        if let Some(ref execution_policy) = input.execution_policy {
            q = q.bind(serde_json::to_value(execution_policy).unwrap());
        }
        if let Some(ref execution_state) = input.execution_state {
            q = q.bind(serde_json::to_value(execution_state).unwrap());
        }
        if let Some(ref monitor_notes) = input.monitor_notes {
            q = q.bind(monitor_notes);
        }
        if let Some(monitor_scheduled_by) = input.monitor_scheduled_by {
            q = q.bind(monitor_scheduled_by);
        }
        if let Some(ref execution_workspace_preference) = input.execution_workspace_preference {
            q = q.bind(execution_workspace_preference);
        }
        if let Some(ref execution_workspace_settings) = input.execution_workspace_settings {
            q = q.bind(execution_workspace_settings);
        }
        if let Some(hidden_at) = input.hidden_at {
            q = q.bind(hidden_at);
        }
        if let Some(ref source_trust) = input.source_trust {
            q = q.bind(source_trust);
        }

        let mut issue = q.fetch_one(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?;

        self.sync_issue_associations(
            id,
            input.label_ids.as_deref(),
            input.blocked_by_issue_ids.as_deref(),
        )
        .await?;
        self.load_issue_projections(&mut issue).await?;
        Ok(issue)
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        // Soft delete by setting status to cancelled
        sqlx::query(
            r#"
            UPDATE issues SET status = 'cancelled', cancelled_at = NOW(), updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(())
    }

    async fn search(
        &self,
        company_id: Uuid,
        query: &str,
        pagination: &Pagination,
    ) -> Result<Vec<Issue>, RepositoryError> {
        let mut issues = sqlx::query_as::<_, Issue>(
            r#"
            SELECT * FROM issues
            WHERE company_id = $1
              AND (
                title ILIKE $2
                OR description ILIKE $2
                OR identifier ILIKE $2
              )
            ORDER BY updated_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(company_id)
        .bind(format!("%{}%", query))
        .bind(pagination.limit)
        .bind(pagination.offset)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        for issue in &mut issues {
            self.load_issue_projections(issue).await?;
        }
        Ok(issues)
    }

    async fn get_by_identifier(&self, identifier: &str) -> Result<Option<Issue>, RepositoryError> {
        let mut issue = sqlx::query_as::<_, Issue>(
            r#"
            SELECT * FROM issues WHERE identifier = $1
            "#,
        )
        .bind(identifier)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        if let Some(issue) = issue.as_mut() {
            self.load_issue_projections(issue).await?;
        }
        Ok(issue)
    }

    async fn list_by_parent(
        &self,
        parent_id: Uuid,
        pagination: &Pagination,
    ) -> Result<Vec<Issue>, RepositoryError> {
        let mut issues = sqlx::query_as::<_, Issue>(
            r#"
            SELECT * FROM issues
            WHERE parent_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(parent_id)
        .bind(pagination.limit)
        .bind(pagination.offset)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        for issue in &mut issues {
            self.load_issue_projections(issue).await?;
        }
        Ok(issues)
    }

    async fn get_by_ids(&self, ids: Vec<Uuid>) -> Result<Vec<Issue>, RepositoryError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut issues = sqlx::query_as::<_, Issue>(
            r#"
            SELECT * FROM issues WHERE id = ANY($1)
            "#,
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        for issue in &mut issues {
            self.load_issue_projections(issue).await?;
        }
        Ok(issues)
    }

    async fn list_ancestors(&self, issue_id: Uuid) -> Result<Vec<Issue>, RepositoryError> {
        let mut issues = sqlx::query_as::<_, Issue>(
            r#"WITH RECURSIVE ancestors AS (
                 SELECT id, company_id, project_id, project_workspace_id, goal_id, parent_id,
                        title, name, description, status, priority, work_mode,
                        assignee_agent_id, assignee_user_id, responsible_user_id, source_trust,
                        created_by_agent_id, created_by_user_id, origin_kind, origin_id,
                        origin_run_id, origin_fingerprint, execution_workspace_id,
                        execution_workspace_preference, execution_policy, execution_state,
                        execution_locked_at, execution_run_id, monitor_scheduled_by,
                        monitor_notes, monitor_next_check_at, monitor_last_triggered_at,
                        monitor_attempt_count, hidden_at, created_at, updated_at, identifier
                   FROM issues WHERE id = $1
                  UNION ALL
                 SELECT p.id, p.company_id, p.project_id, p.project_workspace_id, p.goal_id, p.parent_id,
                        p.title, p.name, p.description, p.status, p.priority, p.work_mode,
                        p.assignee_agent_id, p.assignee_user_id, p.responsible_user_id, p.source_trust,
                        p.created_by_agent_id, p.created_by_user_id, p.origin_kind, p.origin_id,
                        p.origin_run_id, p.origin_fingerprint, p.execution_workspace_id,
                        p.execution_workspace_preference, p.execution_policy, p.execution_state,
                        p.execution_locked_at, p.execution_run_id, p.monitor_scheduled_by,
                        p.monitor_notes, p.monitor_next_check_at, p.monitor_last_triggered_at,
                        p.monitor_attempt_count, p.hidden_at, p.created_at, p.updated_at, p.identifier
                   FROM issues p
                   JOIN ancestors a ON p.id = a.parent_id
               )
               SELECT * FROM ancestors WHERE id != $1
               ORDER BY parent_id NULLS LAST"#,
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        for issue in &mut issues {
            self.load_issue_projections(issue).await?;
        }
        Ok(issues)
    }
}
