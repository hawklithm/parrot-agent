//! Board 认领流程（对应任务拆解 §5 阶段二）。
//!
//! 面向自托管首次运行场景：实例内存在一个 local-board 管理员，
//! 通过一次性挑战 token 将控制权转移给真实用户。
//!
//! 提供：
//! - `create_board_claim_challenge`：生成待认领挑战（含 claim secret）
//! - `inspect_board_claim_challenge`：查看挑战详情（校验 claim secret）
//! - `claim_board_ownership`：认领所有权（事务：移除 local admin -> 添加 instance admin -> 所有公司 owner）

use chrono::{Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::{AuthError, AuthResult};

/// 挑战有效期（分钟）。
const BOARD_CLAIM_TTL_MINUTES: i64 = 30;

/// 内存中的活跃 Board 认领挑战（claim secret -> 挑战详情）。
type ClaimStore = DashMap<String, ActiveBoardClaim>;

/// 一次性 claim token 生成。
fn generate_claim_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let mut raw = [0u8; 32];
    rng.fill(&mut raw);
    hex::encode(raw)
}

/// Board 认领挑战对外视图。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimChallenge {
    pub token: String,
    pub local_board_user_id: Uuid,
    pub company_count: usize,
    pub expires_at: chrono::DateTime<Utc>,
    pub consumed: bool,
}

/// 内存中保存的活跃挑战状态。
struct ActiveBoardClaim {
    local_board_user_id: Uuid,
    company_ids: Vec<Uuid>,
    expires_at: chrono::DateTime<Utc>,
    consumed: bool,
}

/// Board 认领服务。
#[derive(Clone)]
pub struct BoardClaimService {
    pool: PgPool,
    claims: std::sync::Arc<ClaimStore>,
}

impl BoardClaimService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            claims: std::sync::Arc::new(ClaimStore::default()),
        }
    }

    /// 生成 Board 认领挑战：定位当前 local-board 管理员与所有公司。
    pub async fn create_board_claim_challenge(&self) -> AuthResult<(ClaimChallenge, String)> {
        let local_board_user_id = self.resolve_local_board_admin().await?;
        let company_ids = self.list_company_ids().await?;

        let token = generate_claim_token();
        let claim_secret = generate_claim_token();
        let expires_at = Utc::now() + Duration::minutes(BOARD_CLAIM_TTL_MINUTES);

        self.claims.insert(
            claim_secret.clone(),
            ActiveBoardClaim {
                local_board_user_id,
                company_ids: company_ids.clone(),
                expires_at,
                consumed: false,
            },
        );

        let challenge = ClaimChallenge {
            token: token.clone(),
            local_board_user_id,
            company_count: company_ids.len(),
            expires_at,
            consumed: false,
        };

        Ok((challenge, claim_secret))
    }

    /// 查看挑战详情，校验 claim secret。
    pub async fn inspect_board_claim_challenge(
        &self,
        claim_secret: &str,
    ) -> AuthResult<ClaimChallenge> {
        let entry = self
            .claims
            .get(claim_secret)
            .ok_or_else(|| AuthError::BadRequest {
                message: "Board claim challenge not found".to_string(),
            })?;

        if entry.expires_at < Utc::now() {
            drop(entry);
            self.claims.remove(claim_secret);
            return Err(AuthError::BadRequest {
                message: "Claim challenge has expired".to_string(),
            });
        }

        Ok(ClaimChallenge {
            token: claim_secret.to_string(),
            local_board_user_id: entry.local_board_user_id,
            company_count: entry.company_ids.len(),
            expires_at: entry.expires_at,
            consumed: entry.consumed,
        })
    }

    /// 认领所有权：在事务中完成角色与成员关系转移。
    pub async fn claim_board_ownership(
        &self,
        user_id: Uuid,
        claim_secret: &str,
    ) -> AuthResult<()> {
        let mut entry = self
            .claims
            .get_mut(claim_secret)
            .ok_or_else(|| AuthError::BadRequest {
                message: "Board claim challenge not found".to_string(),
            })?;

        if entry.expires_at < Utc::now() {
            drop(entry);
            self.claims.remove(claim_secret);
            return Err(AuthError::BadRequest {
                message: "Claim challenge has expired".to_string(),
            });
        }
        if entry.consumed {
            return Err(AuthError::BadRequest {
                message: "Claim challenge already consumed".to_string(),
            });
        }

        let local_board_user_id = entry.local_board_user_id;
        let company_ids = entry.company_ids.clone();
        entry.consumed = true;
        drop(entry);

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AuthError::Internal {
                message: format!("Failed to begin transaction: {}", e),
            })?;

        // 1. 归档 local-board 管理员在所有公司的成员关系
        sqlx::query(
            "UPDATE company_memberships SET status = 'archived', archived_at = NOW(), updated_at = NOW() \
             WHERE principal_type = 'user' AND principal_id = $1",
        )
        .bind(local_board_user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AuthError::Internal {
            message: format!("Failed to archive local board memberships: {}", e),
        })?;

        // 2. 移除 local-board 管理员的 instance_admin 角色
        sqlx::query(
            "DELETE FROM instance_user_roles WHERE user_id = $1 AND role = 'instance_admin'",
        )
        .bind(local_board_user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AuthError::Internal {
            message: format!("Failed to remove local board instance admin: {}", e),
        })?;

        // 3. 将认领用户添加为 instance_admin
        let admin_role = repositories::models::auth::InstanceUserRole::new(
            user_id,
            "instance_admin".to_string(),
            Some(local_board_user_id),
        );
        sqlx::query(
            "INSERT INTO instance_user_roles (id, user_id, role, granted_by_user_id, granted_at, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (user_id, role) DO NOTHING",
        )
        .bind(admin_role.id)
        .bind(admin_role.user_id)
        .bind(&admin_role.role)
        .bind(admin_role.granted_by_user_id)
        .bind(admin_role.granted_at)
        .bind(admin_role.created_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| AuthError::Internal {
            message: format!("Failed to grant instance admin: {}", e),
        })?;

        // 4. 将认领用户添加为所有公司的 owner
        for company_id in &company_ids {
            let membership = repositories::models::authorization::CompanyMembershipRow::new(
                *company_id,
                "user".to_string(),
                user_id,
                "owner".to_string(),
            );
            sqlx::query(
                "INSERT INTO company_memberships (id, company_id, principal_type, principal_id, role, status, joined_at, archived_at, created_at, updated_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
                 ON CONFLICT (company_id, principal_type, principal_id) \
                 DO UPDATE SET role = 'owner', status = 'active', archived_at = NULL, updated_at = NOW()",
            )
            .bind(membership.id)
            .bind(membership.company_id)
            .bind(&membership.principal_type)
            .bind(membership.principal_id)
            .bind(&membership.role)
            .bind(&membership.status)
            .bind(membership.joined_at)
            .bind(membership.archived_at)
            .bind(membership.created_at)
            .bind(membership.updated_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| AuthError::Internal {
                message: format!("Failed to create owner membership: {}", e),
            })?;
        }

        tx.commit().await.map_err(|e| AuthError::Internal {
            message: format!("Failed to commit claim transaction: {}", e),
        })?;

        Ok(())
    }

    /// 首次管理员认领：将指定用户提升为实例管理员。
    ///
    /// 前置条件：实例当前不存在任何 `instance_admin` 角色（即首次运行）。
    /// 若已存在实例管理员，则返回 `BadRequest`。
    pub async fn claim_first_instance_admin(&self, user_id: Uuid) -> AuthResult<()> {
        // 前置条件：当前无任何实例管理员。
        let existing: Option<Uuid> = sqlx::query_scalar(
            "SELECT user_id FROM instance_user_roles WHERE role = 'instance_admin' LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AuthError::Internal {
            message: format!("Failed to check existing instance admin: {}", e),
        })?;

        if existing.is_some() {
            return Err(AuthError::BadRequest {
                message: "Instance already has an administrator".to_string(),
            });
        }

        let admin_role = repositories::models::auth::InstanceUserRole::new(
            user_id,
            "instance_admin".to_string(),
            None,
        );
        sqlx::query(
            "INSERT INTO instance_user_roles (id, user_id, role, granted_by_user_id, granted_at, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (user_id, role) DO NOTHING",
        )
        .bind(admin_role.id)
        .bind(admin_role.user_id)
        .bind(&admin_role.role)
        .bind(admin_role.granted_by_user_id)
        .bind(admin_role.granted_at)
        .bind(admin_role.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::Internal {
            message: format!("Failed to grant first instance admin: {}", e),
        })?;

        Ok(())
    }

    /// 定位当前 local-board 管理员：即持有 instance_admin 角色的用户。
    async fn resolve_local_board_admin(&self) -> AuthResult<Uuid> {
        let user_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT user_id FROM instance_user_roles WHERE role = 'instance_admin' LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AuthError::Internal {
            message: format!("Failed to resolve local board admin: {}", e),
        })?;

        user_id.ok_or_else(|| AuthError::BadRequest {
            message: "No local board admin found to claim".to_string(),
        })
    }

    /// 列出所有公司 ID。
    async fn list_company_ids(&self) -> AuthResult<Vec<Uuid>> {
        let ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM companies")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AuthError::Internal {
                message: format!("Failed to list companies: {}", e),
            })?;
        Ok(ids)
    }
}
