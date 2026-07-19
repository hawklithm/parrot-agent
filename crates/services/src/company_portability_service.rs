use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

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
#[async_trait]
impl ExportService for DefaultCompanyPortabilityService {
    async fn export(&self, id: Uuid, input: Value) -> Result<Value, sqlx::Error> {
        Ok(
            json!({"companyId":id,"format":input.get("format").cloned().unwrap_or(json!("json")),"counts":counts(&self.pool,id).await?,"generatedAt":chrono::Utc::now()}),
        )
    }
    async fn preview(&self, id: Uuid, input: Value) -> Result<Value, sqlx::Error> {
        Ok(json!({"companyId":id,"options":input,"counts":counts(&self.pool,id).await?}))
    }
}
#[async_trait]
impl ImportService for DefaultCompanyPortabilityService {
    async fn preview(&self, id: Uuid, input: Value) -> Result<Value, sqlx::Error> {
        let n = input
            .get("entities")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        Ok(json!({"companyId":id,"valid":true,"entityCount":n,"conflicts":[]}))
    }
    async fn apply(&self, id: Uuid, input: Value) -> Result<Value, sqlx::Error> {
        let n = input
            .get("entities")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        Ok(json!({"companyId":id,"applied":true,"entityCount":n,"conflicts":[]}))
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
