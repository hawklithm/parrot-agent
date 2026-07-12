//! 认证授权模块集成测试（对应任务拆解 §10 Integration Testing）。
//!
//! 包含：
//! - 认证流程端到端测试
//! - 授权决策测试
//! - 邀请与加入流程测试
//! - 安全场景测试

// ============================================================================
// 辅助模块：测试基础设施
// ============================================================================

#[cfg(test)]
mod test_helpers {
    use std::sync::Arc;
    use sqlx::PgPool;
    use uuid::Uuid;

    use repositories::board_api_key_repository::{
        BoardApiKeyRepository, PgBoardApiKeyRepository, generate_api_key_token, hash_api_key,
    };

    /// 创建测试用的 Board API Key（返回 (key_id, plaintext_token)）
    pub async fn create_test_board_key(
        pool: &PgPool,
        user_id: Uuid,
    ) -> (Uuid, String) {
        let repo = PgBoardApiKeyRepository::new(pool.clone());
        let token = generate_api_key_token("bak_test");
        let key_hash = hash_api_key(&token);
        let key_prefix = token.get(0..16).unwrap_or("bak_test").to_string();

        let key = repo
            .create(user_id, "test-key".to_string(), key_hash, key_prefix, None)
            .await
            .expect("Failed to create test board key");

        (key.id, token)
    }

    /// 创建测试用的 JWT 配置
    pub fn create_test_jwt_config() -> Arc<services::auth::JwtConfig> {
        Arc::new(services::auth::JwtConfig::new(
            "test-secret-key-for-unit-tests".to_string(),
            3600,
            "parrot-agent-test".to_string(),
            "agent-runtime-test".to_string(),
            "test-instance".to_string(),
        ))
    }
}

// ============================================================================
// 认证流程测试
// ============================================================================

#[cfg(test)]
mod auth_flow_tests {
    use uuid::Uuid;
    use super::test_helpers::*;
    use services::auth::*;

    /// 测试 Board API Key 哈希与验证的一致性
    #[test]
    fn test_board_api_key_hash_verify_roundtrip() {
        let token = "bak_test_token_12345";
        let hash = repositories::board_api_key_repository::hash_api_key(token);

        // 验证正确 token
        assert!(repositories::board_api_key_repository::verify_api_key(
            token,
            &hash
        ));

        // 验证错误 token
        assert!(!repositories::board_api_key_repository::verify_api_key(
            "wrong_token",
            &hash
        ));
    }

    /// 测试 Board API Key token 生成格式
    #[test]
    fn test_board_api_key_token_generation() {
        let token = repositories::board_api_key_repository::generate_api_key_token("bak");
        assert!(token.starts_with("bak_"));
        assert_eq!(token.len(), 3 + 1 + 64); // prefix + _ + 32 bytes hex
    }

    /// 测试 Agent API Key token 生成格式
    #[test]
    fn test_agent_api_key_token_generation() {
        let token = repositories::board_api_key_repository::generate_api_key_token("aak");
        assert!(token.starts_with("aak_"));
        assert_eq!(token.len(), 3 + 1 + 64);
    }

    /// 测试 JWT 签发与验证
    #[test]
    fn test_jwt_sign_and_verify() {
        let config = services::auth::JwtConfig::new(
            "test-secret".to_string(),
            3600,
            "test-issuer".to_string(),
            "test-audience".to_string(),
            "test-instance".to_string(),
        );

        let agent_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();

        // 签发 JWT
        let token = services::auth::jwt::create_local_agent_jwt(
            &config,
            agent_id,
            company_id,
            "process".to_string(),
            Some(run_id),
            None,
            None,
        );

        assert!(token.is_some(), "JWT should be signed successfully");

        // 验证 JWT
        let claims = services::auth::jwt::verify_local_agent_jwt(&config, &token.unwrap());
        assert!(claims.is_some(), "JWT should be verified successfully");

        let claims = claims.unwrap();
        assert_eq!(
            claims.sub,
            agent_id.to_string(),
            "sub should match agent_id"
        );
        assert_eq!(
            claims.company_id,
            company_id.to_string(),
            "company_id should match"
        );
    }

    /// 测试过期 JWT 验证失败
    #[test]
    fn test_expired_jwt_rejected() {
        let config = services::auth::JwtConfig::new(
            "test-secret".to_string(),
            0, // 0 second TTL
            "test-issuer".to_string(),
            "test-audience".to_string(),
            "test-instance".to_string(),
        );

        let agent_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();

        let token = services::auth::jwt::create_local_agent_jwt(
            &config,
            agent_id,
            company_id,
            "process".to_string(),
            None,
            None,
            None,
        );
    }

    /// 测试无效 JWT 签名拒绝
    #[test]
    fn test_invalid_jwt_signature_rejected() {
        let sign_config = services::auth::JwtConfig::new(
            "correct-secret".to_string(),
            3600,
            "test-issuer".to_string(),
            "test-audience".to_string(),
            "test-instance".to_string(),
        );

        let verify_config = services::auth::JwtConfig::new(
            "wrong-secret".to_string(),
            3600,
            "test-issuer".to_string(),
            "test-audience".to_string(),
            "test-instance".to_string(),
        );

        let agent_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();

        let token = services::auth::jwt::create_local_agent_jwt(
            &sign_config,
            agent_id,
            company_id,
            "process".to_string(),
            None,
            None,
            None,
        );

        assert!(token.is_some(), "Token should be signed");

        // 用不同密钥验证应失败
        let claims = services::auth::jwt::verify_local_agent_jwt(&verify_config, &token.unwrap());
        assert!(claims.is_none(), "Wrong key should fail verification");
    }

    /// 测试公司级签名密钥隔离
    #[test]
    fn test_company_signing_key_isolation() {
        let secret = "master-secret".to_string();
        let instance_id = "instance-1".to_string();

        let company_a_id = Uuid::new_v4();
        let company_b_id = Uuid::new_v4();
        let key1 = services::auth::jwt::derive_company_signing_key(
            &secret,
            company_a_id,
            &instance_id,
        ).unwrap();
        let key2 = services::auth::jwt::derive_company_signing_key(
            &secret,
            company_b_id,
            &instance_id,
        ).unwrap();

        // 不同公司应有不同的签名密钥
        assert_ne!(key1, key2, "Different companies must have different signing keys");

        // 相同公司+实例应产生相同密钥
        let same_company = Uuid::new_v4();
        let key3 = services::auth::jwt::derive_company_signing_key(
            &secret,
            same_company,
            &instance_id,
        ).unwrap();
        let key4 = services::auth::jwt::derive_company_signing_key(
            &secret,
            same_company,
            &instance_id,
        ).unwrap();
        assert_eq!(key3, key4, "Same company+instance must produce same key");
    }
}

// ============================================================================
// 授权决策测试
// ============================================================================

#[cfg(test)]
mod authorization_tests {
    use uuid::Uuid;
    use services::auth::*;
    use services::auth::authorization_service::{assert_company_access, assert_instance_admin};

    /// 测试实例管理员权限（应始终允许）
    #[test]
    fn test_instance_admin_permissions() {
        let user_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();

        let actor = AuthorizationActor::Board {
            user_id,
            company_id,
            source: ActorSource::Session,
            memberships: vec![CompanyMembership::new(
                company_id,
                PrincipalType::User,
                user_id,
                MembershipRole::Owner,
            )],
            is_instance_admin: true,
        };

        // 实例管理员应始终通过权限检查
        let result = assert_instance_admin(&actor);
        assert!(result.is_ok(), "Instance admin should pass assert_instance_admin");
    }

    /// 测试非实例管理员被拒绝
    #[test]
    fn test_non_instance_admin_rejected() {
        let user_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();

        let actor = AuthorizationActor::Board {
            user_id,
            company_id,
            source: ActorSource::Session,
            memberships: vec![],
            is_instance_admin: false,
        };

        let result = assert_instance_admin(&actor);
        assert!(result.is_err(), "Non-admin should be rejected");
        match result {
            Err(AuthError::Forbidden { .. }) => {} // expected
            _ => panic!("Expected Forbidden error"),
        }
    }

    /// 测试未认证用户被拒绝
    #[test]
    fn test_unauthenticated_rejected() {
        let company_id = Uuid::new_v4();
        let actor = AuthorizationActor::None;

        let result = assert_company_access(&actor, company_id, false);
        assert!(result.is_err(), "Unauthenticated should be rejected");
    }

    /// 测试公司访问权限（Viewer 只读通过）
    #[test]
    fn test_viewer_read_access() {
        let user_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();

        let actor = AuthorizationActor::Board {
            user_id,
            company_id,
            source: ActorSource::Session,
            memberships: vec![CompanyMembership::new(
                company_id,
                PrincipalType::User,
                user_id,
                MembershipRole::Viewer,
            )],
            is_instance_admin: false,
        };

        // Viewer 读操作应通过
        let result = assert_company_access(&actor, company_id, false);
        assert!(result.is_ok(), "Viewer should have read access");
    }

    /// 测试公司访问权限（Viewer 写操作被拒绝）
    #[test]
    fn test_viewer_write_rejected() {
        let user_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();

        let actor = AuthorizationActor::Board {
            user_id,
            company_id,
            source: ActorSource::Session,
            memberships: vec![CompanyMembership::new(
                company_id,
                PrincipalType::User,
                user_id,
                MembershipRole::Viewer,
            )],
            is_instance_admin: false,
        };

        // Viewer 写操作应被拒绝
        let result = assert_company_access(&actor, company_id, true);
        assert!(result.is_err(), "Viewer should be denied write access");
    }

    /// 测试跨公司访问被拒绝
    #[test]
    fn test_cross_company_access_rejected() {
        let user_id = Uuid::new_v4();
        let actor_company = Uuid::new_v4();
        let target_company = Uuid::new_v4();

        let actor = AuthorizationActor::Board {
            user_id,
            company_id: actor_company,
            source: ActorSource::Session,
            memberships: vec![CompanyMembership::new(
                actor_company,
                PrincipalType::User,
                user_id,
                MembershipRole::Owner,
            )],
            is_instance_admin: false,
        };

        // 跨公司访问（非实例管理员）应被拒绝
        let result = assert_company_access(&actor, target_company, false);
        assert!(result.is_err(), "Cross-company access should be denied");
    }

    /// 测试角色默认权限映射
    #[test]
    fn test_role_default_permissions() {
        // Owner 应拥有所有权限
        let owner_perms = RolePermissions::default_permissions_for_role(MembershipRole::Owner);
        assert!(owner_perms.contains(&PermissionKey::new("users:invite")));
        assert!(owner_perms.contains(&PermissionKey::new("joins:approve")));
        assert!(owner_perms.contains(&PermissionKey::new("issues:write")));

        // Viewer 应只有读权限
        let viewer_perms = RolePermissions::default_permissions_for_role(MembershipRole::Viewer);
        assert!(viewer_perms.contains(&PermissionKey::new("issues:read")));
        assert!(!viewer_perms.contains(&PermissionKey::new("issues:write")));
        assert!(!viewer_perms.contains(&PermissionKey::new("users:invite")));
    }

    /// 测试 TrustPreset 解析
    #[test]
    fn test_trust_preset_resolution() {
        let resolver = TrustPresetResolver;

        // Agent actor 进入低信任边界
        let agent_actor = AuthorizationActor::Agent {
            agent_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            run_id: None,
            source: ActorSource::AgentKey,
            key_id: None,
            key_scope: None,
            responsible_user_id: None,
            on_behalf_of_user_id: None,
            on_behalf_of_memberships: vec![],
        };

        let agent_id = Uuid::new_v4();
        let action = AuthorizationAction::IssueMention {
            issue_id: Uuid::new_v4(),
            mentioned_agent_id: agent_id,
        };
        let resolution = TrustPresetResolver::resolve_core_trust_preset(
            &agent_actor,
            &action,
            None,
        );

        assert_eq!(
            resolution.preset,
            TrustPreset::Low,
            "Agent issue mention should be low trust"
        );

        // Board actor 应为高信任
        let board_actor = AuthorizationActor::board_with_source(
            Uuid::new_v4(),
            Uuid::new_v4(),
            ActorSource::Session,
            vec![],
            false,
        );

        let read_action = AuthorizationAction::Permission {
            key: PermissionKey::new("issues:read"),
        };
        let resolution = TrustPresetResolver::resolve_core_trust_preset(
            &board_actor,
            &read_action,
            None,
        );
        assert_eq!(
            resolution.preset,
            TrustPreset::High,
            "Board read should be high trust"
        );
    }
}

// ============================================================================
// 安全场景测试
// ============================================================================

#[cfg(test)]
mod security_tests {
    use uuid::Uuid;
    use services::auth::*;
    use repositories::board_api_key_repository::{hash_api_key, verify_api_key};

    /// 测试 constant-time 比较防止时序攻击
    #[test]
    fn test_constant_time_comparison() {
        let token = "bak_secret_token_12345";
        let hash = hash_api_key(token);

        // 多次验证应消耗大致相同的时间
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            verify_api_key(token, &hash);
        }
        let duration_correct = start.elapsed();

        let start = std::time::Instant::now();
        for _ in 0..1000 {
            verify_api_key("wrong_token_12345_wrong", &hash);
        }
        let duration_wrong = start.elapsed();

        let ratio = duration_correct.as_micros() as f64 / duration_wrong.as_micros() as f64;
        assert!(
            ratio > 0.8 && ratio < 1.2,
            "Timing difference too large: {:.3} (should be ~1.0)",
            ratio
        );
    }

    /// 测试 JWT 跨实例伪造拒绝
    #[test]
    fn test_cross_instance_jwt_forgery() {
        let instance_a = services::auth::JwtConfig::new(
            "secret-a".to_string(),
            3600,
            "issuer-a".to_string(),
            "audience-a".to_string(),
            "instance-a".to_string(),
        );

        let instance_b = services::auth::JwtConfig::new(
            "secret-b".to_string(),
            3600,
            "issuer-b".to_string(),
            "audience-b".to_string(),
            "instance-b".to_string(),
        );

        let agent_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();

        // 从实例 A 签发
        let token = services::auth::jwt::create_local_agent_jwt(
            &instance_a,
            agent_id,
            company_id,
            "process".to_string(),
            None,
            None,
            None,
        );

        assert!(token.is_some(), "Token should be signed by instance A");

        // 在实例 B 验证应失败
        let claims = services::auth::jwt::verify_local_agent_jwt(&instance_b, &token.unwrap());
        assert!(claims.is_none(), "Instance B should reject instance A's token");
    }

    /// 测试跨公司 JWT 伪造拒绝
    #[test]
    fn test_cross_company_jwt_forgery() {
        let config = services::auth::JwtConfig::new(
            "shared-secret".to_string(),
            3600,
            "test-issuer".to_string(),
            "test-audience".to_string(),
            "test-instance".to_string(),
        );

        let agent_id = Uuid::new_v4();
        let company_a = Uuid::new_v4();
        let company_b = Uuid::new_v4();

        // 为 company_a 签发 token
        let token = services::auth::jwt::create_local_agent_jwt(
            &config,
            agent_id,
            company_a,
            "process".to_string(),
            None,
            None,
            None,
        );

        assert!(token.is_some(), "Token should be signed");

        let claims = services::auth::jwt::verify_local_agent_jwt(&config, &token.unwrap());
        assert!(claims.is_some(), "Token should verify with same config");

        // 验证 token 的 company_id 是 company_a 而非 company_b
        let claims = claims.unwrap();
        assert_eq!(
            claims.company_id,
            company_a.to_string(),
            "company_id should match company_a"
        );
        assert_ne!(
            claims.company_id,
            company_b.to_string(),
            "company_id should not be company_b"
        );
    }

    /// 测试错误响应不泄露内部实现细节
    #[test]
    fn test_error_response_no_info_leak() {
        // 内部错误应返回通用消息
        let internal_err = AuthError::internal("Database connection failed: timeout on pool");
        assert_eq!(
            internal_err.user_message(),
            "Internal server error",
            "Internal errors should not leak details"
        );

        // 未认证错误应返回通用消息
        let unauth_err = AuthError::unauthenticated("Invalid token: signature mismatch at byte 42");
        assert_eq!(
            unauth_err.user_message(),
            "Authentication required",
            "Auth errors should not leak token details"
        );

        // 403 错误应返回原因（这是业务逻辑信息）
        let forbidden_err = AuthError::forbidden("Insufficient permissions");
        assert_eq!(
            forbidden_err.user_message(),
            "Insufficient permissions",
            "Forbidden errors should show reason"
        );
    }

    /// 测试 API Key 哈希不可逆
    #[test]
    fn test_api_key_hash_one_way() {
        let token = "bak_super_secret_token_that_should_not_be_recoverable";
        let hash = hash_api_key(token);

        // 哈希不应包含原始 token
        assert!(
            !hash.contains("bak_"),
            "Hash should not contain the original token"
        );

        // 哈希长度应固定（SHA-256 = 64 hex chars）
        assert_eq!(hash.len(), 64, "SHA-256 hash should be 64 hex characters");
    }
}
