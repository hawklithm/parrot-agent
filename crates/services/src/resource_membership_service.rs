use models::{ResourceMemberships, MembershipState, AppError};
use sqlx::PgPool;
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use crate::auth::{AuthorizationActor, ActorSource, MembershipStatus};
use std::future::Future;
use std::pin::Pin;

/// Policy decision result
/// Migrated from paperclip: server/src/services/resource-memberships.ts:31-35
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub reason: Option<String>,
    pub source: Option<String>,
}

impl PolicyDecision {
    pub fn allow(source: impl Into<String>) -> Self {
        Self {
            allowed: true,
            reason: None,
            source: Some(source.into()),
        }
    }

    pub fn deny(reason: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: Some(reason.into()),
            source: Some(source.into()),
        }
    }
}

/// Resource membership policy hook input
#[derive(Debug, Clone)]
pub struct PolicyHookInput {
    pub actor: AuthorizationActor,
    pub company_id: Uuid,
    pub user_id: String,
    pub resource_type: String, // "project" | "agent"
    pub resource_id: Uuid,
    pub state: MembershipState,
    pub starred: Option<bool>,
}

/// Policy hook function type
/// Migrated from paperclip: server/src/services/resource-memberships.ts:37-45
pub type PolicyHook = Box<
    dyn Fn(PolicyHookInput) -> Pin<Box<dyn Future<Output = Result<PolicyDecision, AppError>> + Send>>
        + Send
        + Sync,
>;


#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateResourceMembershipInput {
    pub state: Option<String>, // "joined" | "left"
    pub starred: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMembershipUpdateResult {
    pub resource_type: String,      // "project" | "agent"
    pub resource_id: String,         // UUID as string
    pub state: String,               // "joined" | "left"
    pub starred_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub changed: bool,
    pub change_kind: Option<String>, // "joined" | "left" | "starred" | "unstarred"
    pub policy_source: String,       // "oss_default" | "policy_hook" | custom
}

pub struct ResourceMembershipService {
    pool: PgPool,
    policy_hook: Option<PolicyHook>,
}

impl ResourceMembershipService {
    pub fn new(pool: PgPool) -> Self {
        Self { 
            pool,
            policy_hook: None,
        }
    }

    pub fn with_policy_hook(pool: PgPool, policy_hook: PolicyHook) -> Self {
        Self {
            pool,
            policy_hook: Some(policy_hook),
        }
    }

    /// Assert Board user self-membership access
    /// Migrated from paperclip: server/src/services/resource-memberships.ts:105-119
    fn assert_board_self_membership_access(
        actor: &AuthorizationActor,
        company_id: Uuid,
        user_id: &str,
    ) -> Result<(), AppError> {
        // 1. Check actor type and user_id
        let actor_user_id = match actor {
            AuthorizationActor::Board { user_id, .. } => user_id,
            _ => return Err(AppError::Forbidden("Board user access required".to_string())),
        };

        // 2. Check self-access only
        if actor_user_id.to_string() != user_id {
            return Err(AppError::Forbidden(
                "Users may only update their own resource memberships".to_string(),
            ));
        }

        // 3. Check if local_implicit or instance admin (bypass company check)
        if let AuthorizationActor::Board {
            source,
            is_instance_admin,
            ..
        } = actor
        {
            if *source == ActorSource::LocalImplicit || *is_instance_admin {
                return Ok(());
            }
        }

        // 4. Check company membership status
        if let AuthorizationActor::Board { memberships, .. } = actor {
            let membership = memberships.iter().find(|m| m.company_id == company_id);
            match membership {
                Some(m) if m.status == MembershipStatus::Active => Ok(()),
                _ => Err(AppError::Forbidden(
                    "User does not have active company access".to_string(),
                )),
            }
        } else {
            Err(AppError::Forbidden("Board user access required".to_string()))
        }
    }

    /// Evaluate policy hook
    /// Migrated from paperclip: server/src/services/resource-memberships.ts:121-140
    async fn evaluate_policy(
        &self,
        input: PolicyHookInput,
    ) -> Result<PolicyDecision, AppError> {
        match &self.policy_hook {
            None => Ok(PolicyDecision::allow("oss_default")),
            Some(hook) => {
                match hook(input.clone()).await {
                    Ok(decision) => Ok(PolicyDecision {
                        allowed: decision.allowed,
                        reason: decision.reason,
                        source: decision.source.or(Some("policy_hook".to_string())),
                    }),
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            company_id = %input.company_id,
                            resource_type = %input.resource_type,
                            resource_id = %input.resource_id,
                            "resource membership policy hook failed closed"
                        );
                        Ok(PolicyDecision::deny("policy_hook_failed", "policy_hook"))
                    }
                }
            }
        }
    }

    /// Assert mutation allowed (combines access check + policy evaluation)
    /// Migrated from papep: server/src/services/resource-memberships.ts:145-171
    async fn assert_mutation_allowed(
        &self,
        actor: &AuthorizationActor,
        company_id: Uuid,
        user_id: &str,
        resource_type: &str,
        resource_id: Uuid,
        state: MembershipState,
        starred: Option<bool>,
    ) -> Result<PolicyDecision, AppError> {
        // 1. Assert board self-membership access
        Self::assert_board_self_membership_access(actor, company_id, user_id)?;

        // 2. Evaluate policy hook
        let decision = self.evaluate_policy(PolicyHookInput {
        actor: actor.clone(),
            company_id,
            user_id: user_id.to_string(),
            resource_type: resource_type.to_string(),
            resource_id,
            state,
            starred,
        }).await?;

        // 3. Deny if not allowed
        if !decision.allowed {
            tracing::warn!(
                company_id = %company_id,
                user_id = %user_id,
                resource_type = %resource_type,
                resource_id = %resource_id,
                reason = ?decision.reason,
                source = ?decision.source,
                "resource membership mutation denied"
            );
            return Err(AppError::Forbidden("Resource membership policy denied this request".to_string()));
        }

        Ok(decision)
    }

    /// List all resource memberships for a user
    pub async fn list_for_user(
        &self,
        company_id: Uuid,
        user_id: &str,
    ) -> Result<ResourceMemberships, AppError> {
        let user_uuid = user_id
            .parse::<Uuid>()
            .map_err(|_| AppError::BadRequest("Invalid user_id".to_string()))?;

        // Query project memberships with project status check
        // Migrated from paperclip: server/src/services/resource-memberships.ts:176-193
        let project_rows: Vec<(Uuid, MembershipState, Option<chrono::DateTime<chrono::Utc>>, chrono::DateTime<chrono::Utc>, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
            r#"
            SELECT pm.project_id, pm.state, pm.starred_at, pm.updated_at, p.archived_at
            FROM project_memberships pm
            INNER JOIN projects p ON p.id = pm.project_id AND p.company_id = pm.company_id
            WHERE pm.company_id = $1 AND pm.user_id = $2
            "#
        )
        .bind(company_id)
        .bind(user_uuid)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to query project memberships: {e}")))?;

        // Query agent memberships with agent status check
        // Migrated from paperclip: server/src/services/resource-memberships.ts:194-210
        let agent_rows: Vec<(Uuid, MembershipState, Option<chrono::DateTime<chrono::Utc>>, chrono::DateTime<chrono::Utc>, String)> = sqlx::query_as(
            r#"
            SELECT am.agent_id, am.state, am.starred_at, am.updated_at, a.status
            FROM agent_memberships am
            INNER JOIN agents a ON a.id = am.agent_id AND a.company_id = am.company_id
            WHERE am.company_id = $1 AND am.user_id = $2
            "#
        )
        .bind(company_id)
        .bind(user_uuid)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to query agent memberships: {e}")))?;

        // Build project_memberships map and collect update timestamps
        let mut project_memberships = std::collections::HashMap::new();
        let mut project_starred_at = std::collections::HashMap::new();
        let mut update_timestamps = Vec::new();

        for (id, state, starred_at, updated_at, archived_at) in project_rows {
            let id_str = id.to_string();
            project_memberships.insert(id_str.clone(), state);
            
            // Filter starred projects: exclude archived projects (L212)
            if starred_at.is_some() && archived_at.is_none() {
                if let Some(ts) = starred_at {
                    project_starred_at.insert(id_str, ts);
                }
            }
            update_timestamps.push(updated_at);
        }

        // Build agent_memberships map
        let mut agent_memberships = std::collections::HashMap::new();
        let mut agent_starred_at = std::collections::HashMap::new();

        for (id, state, starred_at, updated_at, agent_status) in agent_rows {
            let id_str = id.to_string();
            agent_memberships.insert(id_str.clone(), state);
            
            // Filter starred agents: exclude terminated agents (L213)
            if starred_at.is_some() && agent_status != "terminated" {
                if let Some(ts) = starred_at {
                    agent_starred_at.insert(id_str, ts);
                }
            }
            update_timestamps.push(updated_at);
        }

        // Extract starred IDs (convert to Uuid)
        let starred_project_ids: Vec<Uuid> = project_starred_at.keys()
            .filter_map(|s| s.parse().ok())
            .collect();
        let starred_agent_ids: Vec<Uuid> = agent_starred_at.keys()
            .filter_map(|s| s.parse().ok())
            .collect();

        // Calculate max updated_at (L221-224)
        let updated_at = update_timestamps.into_iter().max();

        Ok(ResourceMemberships {
            project_memberships,
            agent_memberships,
            starred_project_ids,
            starred_agent_ids,
            project_starred_at: Some(project_starred_at),
            agent_starred_at: Some(agent_starred_at),
            updated_at,
        })
    }

    /// Update project membership (join/leave/star/unstar)
    /// Migrated from paperclip: server/src/services/resource-memberships.ts:updateProject
    pub async fn update_project(
        &self,
        actor: &AuthorizationActor,
        company_id: Uuid,
        user_id: &str,
        project_id: Uuid,
        input: UpdateResourceMembershipInput,
    ) -> Result<ResourceMembershipUpdateResult, AppError> {
        let user_uuid = user_id.parse::<Uuid>()
            .map_err(|_| AppError::BadRequest("Invalid user_id".to_string()))?;
        
        // 1. Check if project exists and is not archived
        let project_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1 AND company_id = $2 AND archived_at IS NULL)"
        )
        .bind(project_id)
        .bind(company_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check project: {e}")))?;

        if !project_exists {
            return Err(AppError::NotFound("Project not found or archived".to_string()));
        }

        // 2. Get existing membership - use MembershipState enum
        let existing: Option<(MembershipState, Option<chrono::DateTime<chrono::Utc>>, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            "SELECT state, starred_at, updated_at FROM project_memberships WHERE company_id = $1 AND user_id = $2 AND project_id = $3"
        )
        .bind(company_id)
        .bind(user_uuid)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to query membership: {e}")))?;

        let previous_state = existing.as_ref().map(|(s, _, _)| *s).unwrap_or(MembershipState::Joined);
        let previous_starred_at = existing.as_ref().and_then(|(_, st, _)| *st);
        let previous_updated_at = existing.as_ref().map(|(_, _, u)| *u);


        // 3. Compute next state (parse input state string to enum)
        let next_state = if input.starred == Some(true) {
            MembershipState::Joined
        } else if let Some(state_str) = &input.state {
            if state_str == "left" {
                MembershipState::Left
            } else {
                MembershipState::Joined
            }
        } else {
            previous_state
        };

        // 4. Assert mutation allowed (权限检查 + policy hook)
        let policy_decision = self.assert_mutation_allowed(
            actor,
            company_id,
            user_id,
            "project",
            project_id,
            next_state,
            input.starred,
        ).await?;

        // 5. Compute next starred_at
        let next_starred_at = if next_state == MembershipState::Left {
            None
        } else if input.starred == Some(true) {
            Some(previous_starred_at.unwrap_or_else(chrono::Utc::now))
        } else if input.starred == Some(false) {
            None
        } else {
            previous_starred_at
        };

        // 6. Check if anything changed
        // 5. Check if anything changed
        let state_changed = previous_state != next_state;
        let starred_changed = previous_starred_at != next_starred_at;
        
        if !state_changed && !starred_changed {
            return Ok(ResourceMembershipUpdateResult {
                resource_type: "project".to_string(),
                resource_id: project_id.to_string(),
                state: if next_state == MembershipState::Left { "left" } else { "joined" }.to_string(),
                starred_at: next_starred_at,
                updated_at: previous_updated_at.unwrap_or_else(chrono::Utc::now),
                changed: false,
                change_kind: None,
                policy_source: policy_decision.source.unwrap_or_else(|| "oss_default".to_string()),
            });
        }

        // 6. Upsert membership
        let now = chrono::Utc::now();
        sqlx::query(
            r#"
            INSERT INTO project_memberships (company_id, user_id, project_id, state, starred_at, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
            ON CONFLICT (company_id, user_id, project_id)
            DO UPDATE SET
                state = EXCLUDED.state,
                starred_at = EXCLUDED.starred_at,
                updated_at = NOW()
            "#
        )
        .bind(company_id)
        .bind(user_uuid)
        .bind(project_id)
        .bind(next_state)
        .bind(next_starred_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to upsert membership: {e}")))?;

        // 7. Compute changeKind (aligned with paperclip logic L314-316)
        let next_state_str = if next_state == MembershipState::Left { "left" } else { "joined" };
        let change_kind = if let Some(starred) = input.starred {
            if starred_changed {
                Some(if starred { "starred" } else { "unstarred" })
            } else if state_changed {
                Some(next_state_str)
            } else {
                Some(if next_starred_at.is_some() { "starred" } else { "unstarred" })
            }
        } else if state_changed {
            Some(next_state_str)
        } else {
            Some(if next_starred_at.is_some() { "starred" } else { "unstarred" })
        };

        Ok(ResourceMembershipUpdateResult {
            resource_type: "project".to_string(),
            resource_id: project_id.to_string(),
            state: next_state_str.to_string(),
            starred_at: next_starred_at,
            updated_at: now,
            changed: true,
            change_kind: change_kind.map(String::from),
            policy_source: policy_decision.source.unwrap_or_else(|| "oss_default".to_string()),
        })
    }

    /// Update agent membership (join/leave/star/unstar)
    /// Migrated from paperclip: server/src/services/resource-memberships.ts:updateAgent
    pub async fn update_agent(
        &self,
        actor: &AuthorizationActor,
        company_id: Uuid,
        user_id: &str,
        agent_id: Uuid,
        input: UpdateResourceMembershipInput,
    ) -> Result<ResourceMembershipUpdateResult, AppError> {
        let user_uuid = user_id.parse::<Uuid>()
            .map_err(|_| AppError::BadRequest("Invalid user_id".to_string()))?;

        // 1. Check if agent exists and is not offboarded
        let agent_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM agents WHERE id = $1 AND company_id = $2 AND status != 'offboarded')"
        )
        .bind(agent_id)
        .bind(company_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check agent: {e}")))?;

        if !agent_exists {
            return Err(AppError::NotFound("Agent not found or offboarded".to_string()));
        }

        // 2. Get existing membership - use MembershipState enum
        let existing: Option<(MembershipState, Option<chrono::DateTime<chrono::Utc>>, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            "SELECT state, starred_at, updated_at FROM agent_memberships WHERE company_id = $1 AND user_id = $2 AND agent_id = $3"
        )
        .bind(company_id)
        .bind(user_uuid)
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to query membership: {e}")))?;

        let previous_state = existing.as_ref().map(|(s, _, _)| *s).unwrap_or(MembershipState::Joined);
        let previous_starred_at = existing.as_ref().and_then(|(_, st, _)| *st);
        let previous_updated_at = existing.as_ref().map(|(_, _, u)| *u);

        // 3. Compute next state (parse input state string to enum)
        let next_state = if input.starred == Some(true) {
            MembershipState::Joined
        } else if let Some(state_str) = &input.state {
            if state_str == "left" {
                MembershipState::Left
            } else {
                MembershipState::Joined
            }
        } else {
            previous_state
        };

        // 4. Assert mutation allowed (权限检查 + policy hook)
        let policy_decision = self.assert_mutation_allowed(
            actor,
            company_id,
            user_id,
            "agent",
            agent_id,
            next_state,
            input.starred,
        ).await?;

        // 5. Compute next starred_at

        // 4. Compute next starred_at
        let next_starred_at = if next_state == MembershipState::Left {
            None
        } else if input.starred == Some(true) {
            Some(previous_starred_at.unwrap_or_else(chrono::Utc::now))
        } else if input.starred == Some(false) {
            None
        } else {
            previous_starred_at
        };

        // 5. Check if anything changed
        let state_changed = previous_state != next_state;
        let starred_changed = previous_starred_at != next_starred_at;
        
        let next_state_str = if next_state == MembershipState::Left { "left" } else { "joined" };
        
        if !state_changed && !starred_changed {
            return Ok(ResourceMembershipUpdateResult {
                resource_type: "agent".to_string(),
                resource_id: agent_id.to_string(),
                state: next_state_str.to_string(),
                starred_at: next_starred_at,
                updated_at: previous_updated_at.unwrap_or_else(chrono::Utc::now),
                changed: false,
                change_kind: None,
                policy_source: policy_decision.source.unwrap_or_else(|| "oss_default".to_string()),
            });
        }

        // 6. Upsert membership
        let now = chrono::Utc::now();
        sqlx::query(
            r#"
            INSERT INTO agent_memberships (company_id, user_id, agent_id, state, starred_at, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
            ON CONFLICT (company_id, user_id, agent_id)
            DO UPDATE SET
                state = EXCLUDED.state,
                starred_at = EXCLUDED.starred_at,
                updated_at = NOW()
            "#
        )
        .bind(company_id)
        .bind(user_uuid)
        .bind(agent_id)
        .bind(next_state)  // MembershipState enum
        .bind(next_starred_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to upsert membership: {e}")))?;

        // 7. Compute changeKind (aligned with paperclip logic L407-409)
        let change_kind = if let Some(starred) = input.starred {
            if starred_changed {
                Some(if starred { "starred" } else { "unstarred" })
            } else if state_changed {
                Some(next_state_str)
            } else {
                Some(if next_starred_at.is_some() { "starred" } else { "unstarred" })
            }
        } else if state_changed {
            Some(next_state_str)
        } else {
            Some(if next_starred_at.is_some() { "starred" } else { "unstarred" })
        };

        Ok(ResourceMembershipUpdateResult {
            resource_type: "agent".to_string(),
            resource_id: agent_id.to_string(),
            state: next_state_str.to_string(),
            starred_at: next_starred_at,
            updated_at: now,
            changed: true,
            change_kind: change_kind.map(String::from),
            policy_source: policy_decision.source.unwrap_or_else(|| "oss_default".to_string()),
        })


    }

    /// Log membership change activity to activity_log table
    /// Migrated from paperclip: server/src/routes/resource-memberships.ts:logMembershipChange
    pub async fn log_membership_activity(
        &self,
        company_id: Uuid,
        actor_type: &str,
        actor_id: Uuid,
        agent_id: Option<Uuid>,
        run_id: Option<Uuid>,
        user_id: &str,
        result: &ResourceMembershipUpdateResult,
    ) -> Result<(), AppError> {
        if !result.changed || result.change_kind.is_none() {
            return Ok(()); // No change, skip logging
        }

        let action = format!("resource_membership.{}", result.change_kind.as_ref().unwrap());
        let details = serde_json::json!({
            "userId": user_id,
            "resourceType": result.resource_type,
            "resourceId": result.resource_id,
            "state": result.state,
            "starredAt": result.starred_at,
            "starred": result.starred_at.is_some(),
        });

        sqlx::query(
            r#"
            INSERT INTO activity_log (company_id, actor_type, actor_id, agent_id, run_id, action, entity_type, entity_id, details, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
            "#
        )
        .bind(company_id)
        .bind(actor_type)
        .bind(actor_id)
        .bind(agent_id)
        .bind(run_id)
        .bind(action)
        .bind(&result.resource_type)
        .bind(Uuid::parse_str(&result.resource_id).unwrap_or_default())
        .bind(details)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Failed to log membership activity: {}", e);
            AppError::Internal(format!("Failed to log activity: {e}"))
        })?;

        Ok(())
    }
}
