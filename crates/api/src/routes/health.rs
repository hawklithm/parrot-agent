use crate::app_state::AppState;
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

/// `/health` 的响应体（对齐 Paperclip `routes/health.ts` 的**脱敏**分支）。
///
/// Paperclip 按调用方是否具备完整视野在「脱敏」与「全量」两套字段间切换；
/// Parrot 的 `/health` 无需认证即可访问，故只暴露脱敏字段——
/// `deploymentMode`、`deploymentExposure`、`commit`、`bootstrapStatus`、
/// `bootstrapInviteActive`。`authReady` 是 Paperclip E2E
/// （`tests/e2e/multi-user.spec.ts:509-518`）明确断言存在的字段，属全量分支，
/// 但它是布尔常量语义（认证栈是否装配完成），在此一并暴露。
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub status: String,
    /// 部署模式：`local_trusted` | `authenticated` | `cloud_managed`。
    pub deployment_mode: Option<String>,
    pub deployment_exposure: String,
    /// 当前二进制对应的 git SHA；不可用时为 null。
    pub commit: Option<String>,
    /// `ready` | `bootstrap_pending`。
    pub bootstrap_status: String,
    pub bootstrap_invite_active: bool,
    /// 认证栈是否就绪。Parrot 在进程启动时装配认证中间件，
    /// 能响应本请求即代表已就绪。
    pub auth_ready: bool,
}

/// 部署元信息：模式、暴露面、构建 SHA。
#[derive(Debug, Clone, Default)]
pub struct DeploymentInfo {
    pub mode: Option<String>,
    pub exposure: String,
    pub commit: Option<String>,
}

impl DeploymentInfo {
    /// 从环境变量读取部署元信息（`DEPLOYMENT_MODE` / `DEPLOYMENT_EXPOSURE` /
    /// `PAPERCLIP_BUILD_COMMIT`），debug 构建缺省为 `local_trusted`。
    pub fn from_env() -> Self {
        Self {
            mode: std::env::var("DEPLOYMENT_MODE").ok().or_else(|| {
                if cfg!(debug_assertions) {
                    Some("local_trusted".to_string())
                } else {
                    None
                }
            }),
            exposure: std::env::var("DEPLOYMENT_EXPOSURE")
                .ok()
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "local".to_string()),
            commit: std::env::var("PAPERCLIP_BUILD_COMMIT")
                .ok()
                .filter(|value| !value.is_empty()),
        }
    }
}

/// 首管认领状态：`(bootstrap_status, bootstrap_invite_active)`。
///
/// 只有 `authenticated` 模式存在「首管是否已认领」的概念；`cloud_managed` 由
/// 控制面拥有身份，`local_trusted` 无首管概念，二者恒为 `ready`（对齐 Paperclip
/// `health.ts` 对 `isCloudManagedInstance` / `local_trusted` 的短路）。
pub async fn resolve_bootstrap_status(
    pool: &sqlx::PgPool,
    deployment_mode: Option<&str>,
) -> (String, bool) {
    if deployment_mode != Some("authenticated") {
        return ("ready".to_string(), false);
    }

    let admin_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM instance_user_roles WHERE role = 'instance_admin'")
            .fetch_one(pool)
            .await
            .unwrap_or(0);
    if admin_count > 0 {
        return ("ready".to_string(), false);
    }

    let invite_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM invites
         WHERE invite_type = 'bootstrap_ceo'
           AND revoked_at IS NULL
           AND accepted = false
           AND expires_at > NOW()",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    ("bootstrap_pending".to_string(), invite_count > 0)
}

/// `GET /api/health` —— 系统健康检查。
///
/// 对齐 Paperclip `routes/health.ts:129-282`：
/// - 数据库探活（`SELECT 1`）失败 → **503** + `status: "unhealthy"`；
/// - `authenticated` 模式下若无 `instance_admin` 角色 → `bootstrap_pending`，
///   并附带是否仍有活跃的 `bootstrap_ceo` 邀请；
/// - `local_trusted` / `cloud_managed` 模式恒为 `ready`。
pub async fn health_check(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    health_response(&state.pool, DeploymentInfo::from_env()).await
}

/// 组装健康响应；抽成显式参数的函数以便按部署模式与连接池状态直接测试。
pub async fn health_response(
    pool: &sqlx::PgPool,
    deployment: DeploymentInfo,
) -> (StatusCode, Json<HealthResponse>) {
    let DeploymentInfo {
        mode: deployment_mode,
        exposure: deployment_exposure,
        commit,
    } = deployment;

    // 数据库探活失败时返回 503，与 Paperclip 的 `database_unreachable` 分支一致。
    if sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool)
        .await
        .is_err()
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "unhealthy".to_string(),
                deployment_mode,
                deployment_exposure,
                commit,
                bootstrap_status: "ready".to_string(),
                bootstrap_invite_active: false,
                auth_ready: false,
            }),
        );
    }

    let (bootstrap_status, bootstrap_invite_active) =
        resolve_bootstrap_status(pool, deployment_mode.as_deref()).await;
 
    (
        StatusCode::OK,
        Json(HealthResponse {
            status: "ok".to_string(),
            deployment_mode,
            deployment_exposure,
            commit,
            bootstrap_status,
            bootstrap_invite_active,
            auth_ready: true,
        }),
    )
 }
