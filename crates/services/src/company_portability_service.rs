use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::portable_path::PortablePath;

#[async_trait]
pub trait ExportService: Send + Sync {
    async fn export(&self, company_id: Uuid, input: Value) -> Result<Value, sqlx::Error>;
    async fn preview(&self, company_id: Uuid, input: Value) -> Result<Value, sqlx::Error>;
}
#[async_trait]
pub trait ImportService: Send + Sync {
    async fn preview(&self, company_id: Uuid, input: Value) -> Result<Value, sqlx::Error>;
    async fn apply(&self, company_id: Uuid, input: Value) -> Result<Value, sqlx::Error>;
}
#[async_trait]
pub trait InboxService: Send + Sync {
    async fn dismiss(&self, company_id: Uuid, input: Value) -> Result<Value, sqlx::Error>;
}
pub struct DefaultCompanyPortabilityService {
    pool: PgPool,
}
impl DefaultCompanyPortabilityService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
async fn counts(pool: &PgPool, company_id: Uuid) -> Result<Value, sqlx::Error> {
    let c=sqlx::query("SELECT (SELECT COUNT(*) FROM issues WHERE company_id=$1) issues, (SELECT COUNT(*) FROM agents WHERE company_id=$1) agents, (SELECT COUNT(*) FROM projects WHERE company_id=$1) projects").bind(company_id).fetch_one(pool).await?;
    Ok(
        json!({"issues":c.get::<i64,_>("issues"),"agents":c.get::<i64,_>("agents"),"projects":c.get::<i64,_>("projects")}),
    )
}

/// Parse the Paperclip `include` object (company/agents/projects/issues/
/// skills booleans), defaulting to everything included.
fn include_flags(input: &Value) -> (bool, bool, bool, bool, bool, bool) {
    let include = input.get("include").and_then(Value::as_object);
    let flag = |key: &str, default: bool| -> bool {
        include
            .and_then(|m| m.get(key))
            .and_then(Value::as_bool)
            .unwrap_or(default)
    };
    (
        flag("company", true),
        flag("agents", true),
        flag("projects", true),
        flag("issues", true),
        flag("skills", true),
        flag("work_products", true),
    )
}

/// Extract the inline source root path and reject paths that escape the
/// portable workspace (absolute paths or `..` traversal). Mirrors the
/// Paperclip portable-path normalisation used for import roots.
fn validated_root_path(input: &Value) -> Result<Option<String>, String> {
    let source = input.get("source").and_then(Value::as_object);
    let root_path = source
        .and_then(|s| s.get("rootPath"))
        .and_then(Value::as_str)
        .or_else(|| input.get("rootPath").and_then(Value::as_str));
    let Some(root) = root_path else {
        return Ok(None);
    };
    let portable = PortablePath::new(root);
    if portable.is_absolute() {
        return Err("Import root path must be relative".into());
    }
    let normalized = portable.to_portable_string();
    if normalized.starts_with("../") || normalized == ".." {
        return Err("Import root path must stay inside the workspace".into());
    }
    Ok(Some(normalized))
}

/// Read the entity arrays from either the Paperclip-style `source.files`
/// (a `manifest.json` entry) or the direct `entities` shape.
fn entity_arrays(input: &Value) -> (Vec<&Value>, Vec<&Value>, Vec<&Value>, Vec<&Value>) {
    let manifest = input
        .get("source")
        .and_then(Value::as_object)
        .and_then(|s| s.get("files"))
        .and_then(Value::as_object)
        .and_then(|files| files.get("manifest.json"))
        .and_then(|v| v.get("manifest").or_else(|| Some(v)));
    let container = input
        .get("entities")
        .or_else(|| manifest.map(|m| m.get("entities")).unwrap_or(None))
        .or_else(|| manifest);
    let container = match container {
        Some(Value::Object(map)) => map,
        _ => return (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
    };
    let agents = container
        .get("agents")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    let projects = container
        .get("projects")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    let issues = container
        .get("issues")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    let work_products = container
        .get("workProducts")
        .or_else(|| container.get("work_products"))
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    (agents, projects, issues, work_products)
}

fn entity_name(entry: &Value) -> Option<String> {
    entry
        .get("name")
        .or_else(|| entry.get("slug"))
        .or_else(|| entry.get("title"))
        .or_else(|| entry.get("identifier"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

#[async_trait]
impl ExportService for DefaultCompanyPortabilityService {
    async fn export(&self, id: Uuid, input: Value) -> Result<Value, sqlx::Error> {
        let (include_company, include_agents, include_projects, include_issues, include_skills, include_work_products) =
            include_flags(&input);
        let company: Option<Value> = if include_company {
            sqlx::query(
                "SELECT name, issue_prefix, budget_monthly_cents, created_at FROM companies WHERE id = $1",
            )
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .map(|r| {
                json!({
                    "id": id,
                    "name": r.get::<String, _>("name"),
                    "issuePrefix": r.get::<String, _>("issue_prefix"),
                    "budgetMonthlyCents": r.get::<i64, _>("budget_monthly_cents"),
                    "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                })
            })
        } else {
            None
        };

        let agents: Vec<Value> = if include_agents {
            let rows = sqlx::query(
                "SELECT id, name, role, status, adapter_type, metadata, created_at \
                 FROM agents WHERE company_id = $1 AND status <> 'terminated' ORDER BY name",
            )
            .bind(id)
            .fetch_all(&self.pool)
            .await?;
            rows.iter()
                .map(|r| {
                    json!({
                        "id": r.get::<Uuid, _>("id"),
                        "name": r.get::<String, _>("name"),
                        "role": r.get::<String, _>("role"),
                        "status": r.get::<String, _>("status"),
                        "adapterType": r.get::<String, _>("adapter_type"),
                        "metadata": r.get::<Value, _>("metadata"),
                        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                    })
                })
                .collect()
        } else {
            Vec::new()
        };

        let projects: Vec<Value> = if include_projects {
            let rows = sqlx::query(
                "SELECT id, name, goal_id, created_at FROM projects WHERE company_id = $1 ORDER BY name",
            )
            .bind(id)
            .fetch_all(&self.pool)
            .await?;
            rows.iter()
                .map(|r| {
                    json!({
                        "id": r.get::<Uuid, _>("id"),
                        "name": r.get::<String, _>("name"),
                        "goalId": r.get::<Option<Uuid>, _>("goal_id"),
                        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                    })
                })
                .collect()
        } else {
            Vec::new()
        };

        let issues: Vec<Value> = if include_issues {
            let rows = sqlx::query(
                "SELECT id, identifier, title, status::text AS status, priority::text AS priority, \
                        assignee_agent_id, created_at \
                 FROM issues WHERE company_id = $1 ORDER BY identifier NULLS LAST, created_at",
            )
            .bind(id)
            .fetch_all(&self.pool)
            .await?;
            rows.iter()
                .map(|r| {
                    json!({
                        "id": r.get::<Uuid, _>("id"),
                        "identifier": r.get::<Option<String>, _>("identifier"),
                        "title": r.get::<String, _>("title"),
                        "status": r.get::<String, _>("status"),
                        "priority": r.get::<Option<String>, _>("priority"),
                        "assigneeAgentId": r.get::<Option<Uuid>, _>("assignee_agent_id"),
                        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                    })
                })
                .collect()
        } else {
            Vec::new()
        };

        // Routines are a Parrot-owned surface (Paperclip carries them through
        // the skills/automations includes); we export them under `routines`.
        let routine_rows = sqlx::query(
            "SELECT id, name, description, status::text AS status, created_at \
             FROM routines WHERE company_id = $1 ORDER BY name",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        let routines: Vec<Value> = routine_rows
            .iter()
            .map(|r| {
                json!({
                    "id": r.get::<Uuid, _>("id"),
                    "name": r.get::<String, _>("name"),
                    "description": r.get::<Option<String>, _>("description"),
                    "status": r.get::<String, _>("status"),
                    "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                })
            })
            .collect();

        let work_products: Vec<Value> = if include_work_products {
            let rows = sqlx::query(
                "SELECT id, issue_id, type, provider, title, status, review_state,                  is_primary, health_status, metadata, source_trust, created_at                  FROM issue_work_products WHERE company_id = $1 ORDER BY id",
            )
            .bind(id)
            .fetch_all(&self.pool)
            .await?;
            rows.iter()
                .map(|r| {
                    json!({
                        "id": r.get::<Uuid, _>("id"),
                        "issueId": r.get::<Uuid, _>("issue_id"),
                        "type": r.get::<String, _>("type"),
                        "provider": r.get::<String, _>("provider"),
                        "title": r.get::<String, _>("title"),
                        "status": r.get::<String, _>("status"),
                        "reviewState": r.get::<String, _>("review_state"),
                        "isPrimary": r.get::<bool, _>("is_primary"),
                        "healthStatus": r.get::<String, _>("health_status"),
                        "metadata": r.get::<Value, _>("metadata"),
                        "sourceTrust": r.get::<Option<Value>, _>("source_trust"),
                        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                    })
                })
                .collect()
        } else {
            Vec::new()
        };

        let skills: Vec<Value> = if include_skills {
            let rows = sqlx::query(
                "SELECT id, name, slug, version, status, category, install_count, created_at \
                 FROM company_skills WHERE company_id = $1 ORDER BY name",
            )
            .bind(id)
            .fetch_all(&self.pool)
            .await?;
            rows.iter()
                .map(|r| {
                    json!({
                        "id": r.get::<Uuid, _>("id"),
                        "name": r.get::<String, _>("name"),
                        "slug": r.get::<String, _>("slug"),
                        "version": r.get::<String, _>("version"),
                        "status": r.get::<String, _>("status"),
                        "category": r.get::<Option<String>, _>("category"),
                        "installCount": r.get::<i32, _>("install_count"),
                        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                    })
                })
                .collect()
        } else {
            Vec::new()
        };

        Ok(json!({
            "companyId": id,
            "format": input.get("format").cloned().unwrap_or(json!("json")),
            "include": {
                "company": include_company,
                "agents": include_agents,
                "projects": include_projects,
                "issues": include_issues,
                "skills": include_skills,
            },
            "company": company,
            "counts": {
                "agents": agents.len(),
                "projects": projects.len(),
                "issues": issues.len(),
                "skills": skills.len(),
                "routines": routines.len(),
            },
            "agents": agents,
            "projects": projects,
            "issues": issues,
            "skills": skills,
            "routines": routines,
            "workProducts": work_products,
            "generatedAt": chrono::Utc::now(),
        }))
    }

    async fn preview(&self, id: Uuid, input: Value) -> Result<Value, sqlx::Error> {
        let (include_company, include_agents, include_projects, include_issues, include_skills, include_work_products) =
            include_flags(&input);
        let c = counts(&self.pool, id).await?;
        let routines: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routines WHERE company_id = $1",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        let skills: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM company_skills WHERE company_id = $1",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        Ok(json!({
            "companyId": id,
            "options": input,
            "counts": {
                "company": include_company,
                "agents": if include_agents { c["agents"].clone() } else { json!(0) },
                "projects": if include_projects { c["projects"].clone() } else { json!(0) },
                "issues": if include_issues { c["issues"].clone() } else { json!(0) },
                "skills": if include_skills { json!(skills) } else { json!(0) },
                "routines": json!(routines),
                "workProducts": sqlx::query_scalar(
                    "SELECT COUNT(*) FROM issue_work_products WHERE company_id = $1",
                )
                .bind(id)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0i64),
            },
        }))
    }
}

fn plan_for(
    existing: &std::collections::HashMap<String, Uuid>,
    entries: &[&Value],
    collision_strategy: &str,
) -> (Vec<Value>, Vec<String>) {
    let mut plans = Vec::new();
    let mut errors = Vec::new();
    for entry in entries {
        let Some(name) = entity_name(entry) else {
            errors.push("Entity entry is missing a name/slug".into());
            continue;
        };
        let action = if existing.contains_key(&name) {
            if collision_strategy == "skip" { "skip" } else { "update" }
        } else {
            "create"
        };
        plans.push(json!({
            "name": name,
            "action": action,
            "existingId": existing.get(&name).map(|id| id.to_string()),
            "reason": if action == "skip" { Some("Already exists and collision strategy is skip".to_string()) } else { None },
        }));
    }
    (plans, errors)
}

async fn load_existing(pool: &PgPool, company_id: Uuid) -> Result<(std::collections::HashMap<String, Uuid>, std::collections::HashMap<String, Uuid>, std::collections::HashMap<String, Uuid>), sqlx::Error> {
    let agent_rows = sqlx::query("SELECT id, name FROM agents WHERE company_id = $1 AND status <> 'terminated'")
        .bind(company_id)
        .fetch_all(pool)
        .await?;
    let mut agents = std::collections::HashMap::new();
    for r in agent_rows {
        agents.insert(r.get::<String, _>("name"), r.get::<Uuid, _>("id"));
    }
    let project_rows = sqlx::query("SELECT id, name FROM projects WHERE company_id = $1")
        .bind(company_id)
        .fetch_all(pool)
        .await?;
    let mut projects = std::collections::HashMap::new();
    for r in project_rows {
        projects.insert(r.get::<String, _>("name"), r.get::<Uuid, _>("id"));
    }
    let issue_rows = sqlx::query("SELECT id, title FROM issues WHERE company_id = $1")
        .bind(company_id)
        .fetch_all(pool)
        .await?;
    let mut issues = std::collections::HashMap::new();
    for r in issue_rows {
        issues.insert(r.get::<String, _>("title"), r.get::<Uuid, _>("id"));
    }
    Ok((agents, projects, issues))
}

#[async_trait]
impl ImportService for DefaultCompanyPortabilityService {
    async fn preview(&self, id: Uuid, input: Value) -> Result<Value, sqlx::Error> {
        let root_path = validated_root_path(&input).map_err(|e| {
            sqlx::Error::Protocol(format!("invalid import root: {e}"))
        })?;
        let (agent_entries, project_entries, issue_entries, work_product_entries) = entity_arrays(&input);
        let (existing_agents, existing_projects, existing_issues) = load_existing(&self.pool, id).await?;
        let collision_strategy = input
            .get("collisionStrategy")
            .and_then(Value::as_str)
            .unwrap_or("skip")
            .to_string();

        let mut errors: Vec<String> = Vec::new();
        if let Some(root) = &root_path {
            // Only reject traversal; a plain relative root is accepted.
            let portable = PortablePath::new(root);
            if portable.components().is_empty() {
                errors.push("Import root path cannot be empty".into());
            }
        }
        let (agent_plans, agent_errors) = plan_for(&existing_agents, &agent_entries, &collision_strategy);
        let (project_plans, project_errors) = plan_for(&existing_projects, &project_entries, &collision_strategy);
        let (issue_plans, issue_errors) = plan_for(&existing_issues, &issue_entries, &collision_strategy);
        errors.extend(agent_errors);
        errors.extend(project_errors);
        errors.extend(issue_errors);

        let conflicts: Vec<Value> = agent_plans
            .iter()
            .chain(project_plans.iter())
            .chain(issue_plans.iter())
            .filter(|p| p["action"] == "update" || p["action"] == "skip")
            .cloned()
            .collect();

        Ok(json!({
            "companyId": id,
            "valid": errors.is_empty(),
            "rootPath": root_path,
            "collisionStrategy": collision_strategy,
            "entityCount": agent_entries.len() + project_entries.len() + issue_entries.len() + work_product_entries.len(),
            "conflicts": conflicts,
            "errors": errors,
            "plan": {
                "companyAction": "update",
                "agentPlans": agent_plans,
                "projectPlans": project_plans,
                "issuePlans": issue_plans,
                "workProductPlans": vec![json!({"action": "create"})],
            },
        }))
    }

    async fn apply(&self, id: Uuid, input: Value) -> Result<Value, sqlx::Error> {
        let root_path = validated_root_path(&input).map_err(|e| {
            sqlx::Error::Protocol(format!("invalid import root: {e}"))
        })?;
        let (agent_entries, project_entries, issue_entries, work_product_entries) = entity_arrays(&input);
        let (existing_agents, existing_projects, existing_issues) = load_existing(&self.pool, id).await?;
        let collision_strategy = input
            .get("collisionStrategy")
            .and_then(Value::as_str)
            .unwrap_or("skip")
            .to_string();

        let mut tx = self.pool.begin().await?;

        let mut agent_results = Vec::new();
        for entry in &agent_entries {
            let Some(name) = entity_name(entry) else { continue };
            let adapter_type = entry
                .get("adapterType")
                .or_else(|| entry.get("adapter_type"))
                .and_then(Value::as_str)
                .unwrap_or("process");
            let (agent_id, action) = match existing_agents.get(&name) {
                Some(id) if collision_strategy == "skip" => (*id, "skipped"),
                Some(id) => {
                    sqlx::query("UPDATE agents SET name = $1, adapter_type = $2, updated_at = NOW() WHERE id = $3")
                        .bind(&name)
                        .bind(adapter_type)
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    (*id, "updated")
                }
                None => {
                    let agent_id = Uuid::new_v4();
                    sqlx::query(
                        "INSERT INTO agents (id, company_id, name, adapter_type) VALUES ($1, $2, $3, $4)",
                    )
                    .bind(agent_id)
                    .bind(id)
                    .bind(&name)
                    .bind(adapter_type)
                    .execute(&mut *tx)
                    .await?;
                    (agent_id, "created")
                }
            };
            agent_results.push(json!({ "name": name, "id": agent_id, "action": action }));
        }

        let mut project_results = Vec::new();
        for entry in &project_entries {
            let Some(name) = entity_name(entry) else { continue };
            let (project_id, action) = match existing_projects.get(&name) {
                Some(id) if collision_strategy == "skip" => (*id, "skipped"),
                Some(id) => {
                    sqlx::query("UPDATE projects SET name = $1 WHERE id = $2")
                        .bind(&name)
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    (*id, "updated")
                }
                None => {
                    let project_id = Uuid::new_v4();
                    sqlx::query("INSERT INTO projects (id, company_id, name) VALUES ($1, $2, $3)")
                        .bind(project_id)
                        .bind(id)
                        .bind(&name)
                        .execute(&mut *tx)
                        .await?;
                    (project_id, "created")
                }
            };
            project_results.push(json!({ "name": name, "id": project_id, "action": action }));
        }

        let mut issue_results = Vec::new();
        for entry in &issue_entries {
            let Some(title) = entity_name(entry) else { continue };
            let status = entry
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("backlog");
            let (issue_id, action) = match existing_issues.get(&title) {
                Some(id) if collision_strategy == "skip" => (*id, "skipped"),
                Some(id) => {
                    sqlx::query("UPDATE issues SET title = $1, status = $2::issue_status WHERE id = $3")
                        .bind(&title)
                        .bind(status)
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    (*id, "updated")
                }
                None => {
                    let issue_id = Uuid::new_v4();
                    sqlx::query(
                        "INSERT INTO issues (id, company_id, title, status) VALUES ($1, $2, $3, $4::issue_status)",
                    )
                    .bind(issue_id)
                    .bind(id)
                    .bind(&title)
                    .bind(status)
                    .execute(&mut *tx)
                    .await?;
                    (issue_id, "created")
                }
            };
            issue_results.push(json!({ "title": title, "id": issue_id, "action": action }));
        }

        // Import work products (linked to issues via issue_id)
        let mut work_product_results = Vec::new();
        for entry in work_product_entries {
            let Some(_id) = entry.get("id").and_then(Value::as_str).map(|s| s.to_string()) else { continue };
            let issue_id = entry
                .get("issueId")
                .or_else(|| entry.get("issue_id"))
                .and_then(Value::as_str)
                .and_then(|s| Uuid::parse_str(s).ok())
                .unwrap_or(Uuid::nil());
            if issue_id.is_nil() { continue; }

            let wp_id = Uuid::new_v4();
            let type_val = entry.get("type").and_then(Value::as_str).unwrap_or("artifact");
            let provider = entry.get("provider").and_then(Value::as_str).unwrap_or("parrot");
            let title = entry.get("title").and_then(Value::as_str).unwrap_or("work product");
            let status = entry.get("status").and_then(Value::as_str).unwrap_or("active");
            let review_state = entry.get("reviewState").or_else(|| entry.get("review_state"))
                .and_then(Value::as_str).unwrap_or("none");
            let is_primary = entry.get("isPrimary").or_else(|| entry.get("is_primary"))
                .and_then(Value::as_bool).unwrap_or(false);
            let health_status = entry.get("healthStatus").or_else(|| entry.get("health_status"))
                .and_then(Value::as_str).unwrap_or("unknown");
            let metadata = entry.get("metadata").cloned();
            let source_trust = entry.get("sourceTrust").or_else(|| entry.get("source_trust")).cloned();

            sqlx::query(
                "INSERT INTO issue_work_products                  (id, company_id, issue_id, type, provider, title, status, review_state,                   is_primary, health_status, metadata, source_trust, created_at)                  VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW())",
            )
            .bind(wp_id)
            .bind(id)
            .bind(issue_id)
            .bind(type_val)
            .bind(provider)
            .bind(title)
            .bind(status)
            .bind(review_state)
            .bind(is_primary)
            .bind(health_status)
            .bind(metadata)
            .bind(source_trust)
            .execute(&mut *tx)
            .await?;

            work_product_results.push(json!({ "id": wp_id, "issueId": issue_id, "action": "created" }));
        }

        tx.commit().await?;

        Ok(json!({
            "companyId": id,
            "applied": true,
            "rootPath": root_path,
            "entityCount": agent_results.len() + project_results.len() + issue_results.len() + work_product_results.len(),
            "agents": agent_results,
            "projects": project_results,
            "issues": issue_results,
            "workProducts": work_product_results,
        }))
    }
}
#[async_trait]
impl InboxService for DefaultCompanyPortabilityService {
    async fn dismiss(&self, id: Uuid, input: Value) -> Result<Value, sqlx::Error> {
        let issue = input
            .get("issueId")
            .or_else(|| input.get("issue_id"))
            .and_then(Value::as_str)
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or(Uuid::nil());
        let user = input
            .get("userId")
            .or_else(|| input.get("user_id"))
            .and_then(Value::as_str)
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or(Uuid::nil());
        let row=sqlx::query("INSERT INTO issue_inbox_archives(id,company_id,issue_id,user_id,archived_at) VALUES($1,$2,$3,$4,NOW()) ON CONFLICT(company_id,issue_id,user_id) DO UPDATE SET archived_at=NOW(),updated_at=NOW() RETURNING id,archived_at,updated_at").bind(Uuid::new_v4()).bind(id).bind(issue).bind(user).fetch_one(&self.pool).await?;
        Ok(
            json!({"id":row.get::<Uuid,_>("id"),"companyId":id,"issueId":issue,"userId":user,"archivedAt":row.get::<chrono::DateTime<chrono::Utc>,_>("archived_at"),"updatedAt":row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")}),
        )
    }
}
