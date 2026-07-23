use models::{
    AgentMembership, AgentMembershipWithAgent, CreateProjectInput, CreateWorkspaceInput,
    MembershipState, Project, ProjectMembership, ProjectMembershipWithProject, ProjectWorkspace,
    ResourceMemberships, UpdateProjectInput,
};
use sqlx::{PgPool, Result};
use uuid::Uuid;

pub struct ProjectRepository {
    pool: PgPool,
}

impl ProjectRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, input: CreateProjectInput) -> Result<Project> {
        sqlx::query_as::<_, Project>(
            r#"
            INSERT INTO projects (
                company_id, goal_id, name, description, lead_agent_id,
                status, target_date, color, icon, env, execution_workspace_policy
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING *
            "#,
        )
        .bind(input.company_id)
        .bind(input.goal_id.or_else(|| input.goal_ids.and_then(|ids| ids.into_iter().next())))
        .bind(&input.name)
        .bind(&input.description)
        .bind(input.lead_agent_id)
        .bind(input.status.unwrap_or(models::ProjectStatus::Backlog))
        .bind(input.target_date)
        .bind(&input.color)
        .bind(&input.icon)
        .bind(&input.env)
        .bind(
            input
                .execution_workspace_policy
                .unwrap_or(models::ExecutionWorkspacePolicy::Shared),
        )
        .fetch_one(&self.pool)
        .await
    }

    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<Project>> {
        sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn list_by_company(&self, company_id: Uuid) -> Result<Vec<Project>> {
        sqlx::query_as::<_, Project>(
            "SELECT * FROM projects WHERE company_id = $1 ORDER BY created_at DESC",
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn update(&self, id: Uuid, input: UpdateProjectInput) -> Result<Project> {
        let mut query = String::from("UPDATE projects SET updated_at = NOW()");
        let mut bind_count = 1;

        if input.name.is_some() {
            query.push_str(&format!(", name = ${}", bind_count));
            bind_count += 1;
        }
        if input.description.is_some() {
            query.push_str(&format!(", description = ${}", bind_count));
            bind_count += 1;
        }
        if input.status.is_some() {
            query.push_str(&format!(", status = ${}", bind_count));
            bind_count += 1;
        }
        if input.lead_agent_id.is_some() {
            query.push_str(&format!(", lead_agent_id = ${}", bind_count));
            bind_count += 1;
        }
        if input.target_date.is_some() {
            query.push_str(&format!(", target_date = ${}", bind_count));
            bind_count += 1;
        }
        if input.color.is_some() {
            query.push_str(&format!(", color = ${}", bind_count));
            bind_count += 1;
        }
        if input.icon.is_some() {
            query.push_str(&format!(", icon = ${}", bind_count));
            bind_count += 1;
        }
        if input.env.is_some() {
            query.push_str(&format!(", env = ${}", bind_count));
            bind_count += 1;
        }
        if input.pause_reason.is_some() {
            query.push_str(&format!(", pause_reason = ${}", bind_count));
            bind_count += 1;
        }
        if input.execution_workspace_policy.is_some() {
            query.push_str(&format!(", execution_workspace_policy = ${}", bind_count));
            bind_count += 1;
        }

        query.push_str(&format!(" WHERE id = ${} RETURNING *", bind_count));

        let mut q = sqlx::query_as::<_, Project>(&query);

        if let Some(name) = input.name {
            q = q.bind(name);
        }
        if let Some(description) = input.description {
            q = q.bind(description);
        }
        if let Some(status) = input.status {
            q = q.bind(status);
        }
        if let Some(lead_agent_id) = input.lead_agent_id {
            q = q.bind(lead_agent_id);
        }
        if let Some(target_date) = input.target_date {
            q = q.bind(target_date);
        }
        if let Some(color) = input.color {
            q = q.bind(color);
        }
        if let Some(icon) = input.icon {
            q = q.bind(icon);
        }
        if let Some(env) = input.env {
            q = q.bind(env);
        }
        if let Some(pause_reason) = input.pause_reason {
            q = q.bind(pause_reason);
        }
        if let Some(policy) = input.execution_workspace_policy {
            q = q.bind(policy);
        }

        q.bind(id).fetch_one(&self.pool).await
    }

    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM projects WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn archive(&self, id: Uuid) -> Result<Project> {
        sqlx::query_as::<_, Project>(
            "UPDATE projects SET archived_at = NOW(), updated_at = NOW() WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
    }

    // Workspace operations
    pub async fn create_workspace(&self, input: CreateWorkspaceInput) -> Result<ProjectWorkspace> {
        sqlx::query_as::<_, ProjectWorkspace>(
            r#"
            INSERT INTO project_workspaces (project_id, name, config, is_primary)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
        )
        .bind(input.project_id)
        .bind(&input.name)
        .bind(&input.config)
        .bind(input.is_primary.unwrap_or(false))
        .fetch_one(&self.pool)
        .await
    }

    pub async fn list_workspaces(&self, project_id: Uuid) -> Result<Vec<ProjectWorkspace>> {
        sqlx::query_as::<_, ProjectWorkspace>(
            "SELECT * FROM project_workspaces WHERE project_id = $1 ORDER BY is_primary DESC, created_at ASC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn update_workspace(
        &self,
        project_id: Uuid,
        workspace_id: Uuid,
        name: Option<String>,
        config: Option<serde_json::Value>,
        is_primary: Option<bool>,
    ) -> Result<Option<ProjectWorkspace>> {
        sqlx::query_as::<_, ProjectWorkspace>(
            "UPDATE project_workspaces SET name = COALESCE($3, name), config = COALESCE($4, config), is_primary = COALESCE($5, is_primary), updated_at = NOW() WHERE id = $1 AND project_id = $2 RETURNING *",
        )
        .bind(workspace_id)
        .bind(project_id)
        .bind(name)
        .bind(config)
        .bind(is_primary)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn external_object_summary(&self, project_id: Uuid) -> Result<serde_json::Value> {
        let row = sqlx::query(
            "SELECT (SELECT COUNT(*) FROM issues WHERE project_id = $1) AS issues, (SELECT COUNT(*) FROM agents a JOIN projects p ON p.company_id = a.company_id WHERE p.id = $1) AS agents, (SELECT COUNT(*) FROM project_workspaces WHERE project_id = $1) AS workspaces",
        )
        .bind(project_id)
        .fetch_one(&self.pool)
        .await?;
        use sqlx::Row;
        Ok(serde_json::json!({
            "projectId": project_id,
            "issues": row.get::<i64, _>("issues"),
            "agents": row.get::<i64, _>("agents"),
            "workspaces": row.get::<i64, _>("workspaces"),
        }))
    }

    pub async fn get_primary_workspace(
        &self,
        project_id: Uuid,
    ) -> Result<Option<ProjectWorkspace>> {
        sqlx::query_as::<_, ProjectWorkspace>(
            "SELECT * FROM project_workspaces WHERE project_id = $1 AND is_primary = true",
        )
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn delete_workspace(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM project_workspaces WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // Membership operations
    pub async fn upsert_project_membership(
        &self,
        company_id: Uuid,
        project_id: Uuid,
        user_id: Uuid,
        state: MembershipState,
    ) -> Result<ProjectMembership> {
        sqlx::query_as::<_, ProjectMembership>(
            r#"
            INSERT INTO project_memberships (company_id, project_id, user_id, state)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (company_id, project_id, user_id)
            DO UPDATE SET state = $4, updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(company_id)
        .bind(project_id)
        .bind(user_id)
        .bind(state)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn upsert_agent_membership(
        &self,
        company_id: Uuid,
        agent_id: Uuid,
        user_id: Uuid,
        state: MembershipState,
        starred: Option<bool>,
    ) -> Result<AgentMembership> {
        // `starred` semantics mirror Paperclip updateAgent:
        //  - Some(true)  -> starred_at = COALESCE(starred_at, NOW())
        //  - Some(false) -> starred_at = NULL
        //  - None        -> leave starred_at untouched
        sqlx::query_as::<_, AgentMembership>(
            r#"
            INSERT INTO agent_memberships (company_id, agent_id, user_id, state, starred_at)
            VALUES ($1, $2, $3, $4,
                    CASE WHEN $5 = TRUE THEN NOW()
                         WHEN $5 = FALSE THEN NULL
                         ELSE NULL
                    END)
            ON CONFLICT (company_id, agent_id, user_id)
            DO UPDATE SET state = EXCLUDED.state,
                          starred_at = CASE
                            WHEN $5 = TRUE THEN COALESCE(agent_memberships.starred_at, NOW())
                            WHEN $5 = FALSE THEN NULL
                            ELSE agent_memberships.starred_at
                          END,
                          updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(company_id)
        .bind(agent_id)
        .bind(user_id)
        .bind(state)
        .bind(starred)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn star_project(
        &self,
        company_id: Uuid,
        project_id: Uuid,
        user_id: Uuid,
    ) -> Result<ProjectMembership> {
        sqlx::query_as::<_, ProjectMembership>(
            r#"
            INSERT INTO project_memberships (company_id, project_id, user_id, state, starred_at)
            VALUES ($1, $2, $3, 'joined', NOW())
            ON CONFLICT (company_id, project_id, user_id)
            DO UPDATE SET starred_at = NOW(), updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(company_id)
        .bind(project_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn unstar_project(
        &self,
        company_id: Uuid,
        project_id: Uuid,
        user_id: Uuid,
    ) -> Result<ProjectMembership> {
        sqlx::query_as::<_, ProjectMembership>(
            r#"
            UPDATE project_memberships
            SET starred_at = NULL, updated_at = NOW()
            WHERE company_id = $1 AND project_id = $2 AND user_id = $3
            RETURNING *
            "#,
        )
        .bind(company_id)
        .bind(project_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn list_memberships_for_user(
        &self,
        company_id: Uuid,
        user_id: Uuid,
    ) -> Result<ResourceMemberships> {
        // Fetch project memberships separately
        let memberships = sqlx::query_as::<_, ProjectMembership>(
            r#"
            SELECT *
            FROM project_memberships
            WHERE company_id = $1 AND user_id = $2 AND state = 'joined'
            ORDER BY starred_at DESC NULLS LAST, created_at DESC
            "#,
        )
        .bind(company_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let mut project_memberships = Vec::new();
        for membership in memberships {
            if let Some(project) = self.get_by_id(membership.project_id).await? {
                project_memberships.push(ProjectMembershipWithProject {
                    membership,
                    project,
                });
            }
        }

        // Fetch agent memberships separately
        let agent_memberships_raw = sqlx::query_as::<_, AgentMembership>(
            r#"
            SELECT *
            FROM agent_memberships
            WHERE company_id = $1 AND user_id = $2 AND state = 'joined'
            ORDER BY starred_at DESC NULLS LAST, created_at DESC
            "#,
        )
        .bind(company_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let mut agent_memberships = Vec::new();
        for membership in agent_memberships_raw {
            let agent_info: Option<(String, String)> =
                sqlx::query_as(r#"SELECT name, status FROM agents WHERE id = $1"#)
                    .bind(membership.agent_id)
                    .fetch_optional(&self.pool)
                    .await?;

            if let Some((agent_name, agent_status)) = agent_info {
                agent_memberships.push(AgentMembershipWithAgent {
                    membership,
                    agent_name,
                    agent_status,
                });
            }
        }

        let starred_project_ids: Vec<Uuid> = project_memberships
            .iter()
            .filter(|pm| pm.membership.starred_at.is_some())
            .map(|pm| pm.membership.project_id)
            .collect();

        let starred_agent_ids: Vec<Uuid> = agent_memberships
            .iter()
            .filter(|am| am.membership.starred_at.is_some())
            .map(|am| am.membership.agent_id)
            .collect();

        Ok(ResourceMemberships {
            project_memberships,
            agent_memberships,
            starred_project_ids,
            starred_agent_ids,
        })
    }
}
