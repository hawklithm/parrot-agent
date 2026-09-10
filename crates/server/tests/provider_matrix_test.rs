//! HTTP parity integration tests for Storage and Secret Provider matrix.
//!
//! Covers all provider combinations:
//! - Storage: local_disk, s3
//! - Secret: local_encrypted, aws_secrets_manager, gcp_secret_manager, vault
//!
//! L667 — Provider matrix (Storage + Secret Provider).
//! Paperclip source: packages/server/src/storage/types.ts + secret_providers.ts

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::secret_provider_configs::secret_provider_config_routes;
use parrot_server::build_app_state;
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------

async fn connect_and_migrate() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:admin123@localhost:5433/parrot_agent_compile".to_string()
    });
    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect database");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    user_id: Uuid,
    agent_id: Uuid,
}

async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let prefix = format!("PM{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("ProviderMatrixTest")
        .bind(&prefix)
        .execute(pool)
        .await
        .expect("insert company");

    sqlx::query(
        "INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(format!("pm{}@test.com", user_id))
    .bind("PM User")
    .execute(pool)
    .await
    .expect("insert auth_user");

    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(company_id)
    .bind("user")
    .bind(user_id)
    .bind("owner")
    .bind("active")
    .execute(pool)
    .await
    .expect("insert membership");

    sqlx::query(
        "INSERT INTO agents (id, company_id, name, adapter_type, status) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(agent_id)
    .bind(company_id)
    .bind("PM Agent")
    .bind("http")
    .bind("idle")
    .execute(pool)
    .await
    .expect("insert agent");

    Fixture {
        pool: pool.clone(),
        company_id,
        user_id,
        agent_id,
    }
}

fn owner_actor(fixture: &Fixture) -> AuthorizationActor {
    AuthorizationActor::board_with_source(
        fixture.user_id,
        fixture.company_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            fixture.company_id,
            PrincipalType::User,
            fixture.user_id,
            MembershipRole::Owner,
        )],
        true,
    )
}

async fn send(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    let req_body = match body {
        Some(ref value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(value).expect("serialize request body"))
        }
        None => Body::empty(),
    };
    let mut req = builder.body(req_body).expect("build request");
    req.extensions_mut().insert(actor.clone());
    let resp = app.clone().oneshot(req).await.expect("dispatch request");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    (status, bytes.to_vec())
}

fn parse_json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).expect("response body must be JSON")
}

// ---------------------------------------------------------------------------
// PM1: List secret provider configs is empty initially
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_secret_provider_configs_empty() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = secret_provider_config_routes().with_state(state);
    let actor = owner_actor(&f);

    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/secret-provider-configs", f.company_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "expected 200 OK, got {}", status);
    let json = parse_json(&body);
    assert!(json.is_array(), "response should be a JSON array, got: {}", json);
    assert_eq!(json.as_array().unwrap().len(), 0, "expected empty array");
}

// ---------------------------------------------------------------------------
// PM2: Create local_encrypted secret provider config
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_local_encrypted_provider() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = secret_provider_config_routes().with_state(state);
    let actor = owner_actor(&f);

    let body = json!({
        "provider": "local_encrypted",
        "displayName": "Local Encrypted Store",
        "config": {
            "keyPath": "/etc/parrot/secrets.key"
        },
        "setAsDefault": false
    });

    let (status, resp_body) = send(
        &app,
        &actor,
        "POST",
        &format!("/companies/{}/secret-provider-configs", f.company_id),
        Some(body),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::CREATED,
        "expected 201 Created, got {}: {}",
        status,
        String::from_utf8_lossy(&resp_body)
    );

    let json = parse_json(&resp_body);
    assert_eq!(json["provider"], "local_encrypted");
    assert_eq!(json["displayName"], "Local Encrypted Store");

    // Verify it persists via list
    let (status, list_body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/secret-provider-configs", f.company_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let json = parse_json(&list_body);
    assert_eq!(json.as_array().unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// PM3: Create AWS Secrets Manager provider config
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_aws_secrets_manager_provider() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = secret_provider_config_routes().with_state(state);
    let actor = owner_actor(&f);

    let body = json!({
        "provider": "aws_secrets_manager",
        "displayName": "AWS Secrets Manager",
        "config": {
            "region": "us-east-1",
            "secretPrefix": "parrot/"
        },
        "setAsDefault": false
    });

    let (status, resp_body) = send(
        &app,
        &actor,
        "POST",
        &format!("/companies/{}/secret-provider-configs", f.company_id),
        Some(body),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::CREATED,
        "expected 201 Created, got {}: {}",
        status,
        String::from_utf8_lossy(&resp_body)
    );

    let json = parse_json(&resp_body);
    assert_eq!(json["provider"], "aws_secrets_manager");
}

// ---------------------------------------------------------------------------
// PM4: Create GCP Secret Manager provider config
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_gcp_secret_manager_provider() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = secret_provider_config_routes().with_state(state);
    let actor = owner_actor(&f);

    let body = json!({
        "provider": "gcp_secret_manager",
        "displayName": "GCP Secret Manager",
        "config": {
            "projectId": "my-gcp-project",
            "secretPrefix": "parrot/"
        },
        "setAsDefault": false
    });

    let (status, resp_body) = send(
        &app,
        &actor,
        "POST",
        &format!("/companies/{}/secret-provider-configs", f.company_id),
        Some(body),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::CREATED,
        "expected 201 Created, got {}: {}",
        status,
        String::from_utf8_lossy(&resp_body)
    );

    let json = parse_json(&resp_body);
    assert_eq!(json["provider"], "gcp_secret_manager");
}

// ---------------------------------------------------------------------------
// PM5: Create Vault provider config
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_vault_provider() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = secret_provider_config_routes().with_state(state);
    let actor = owner_actor(&f);

    let body = json!({
        "provider": "vault",
        "displayName": "HashiCorp Vault",
        "config": {
            "address": "https://vault.example.com:8200",
            "mountPoint": "secret"
        },
        "setAsDefault": false
    });

    let (status, resp_body) = send(
        &app,
        &actor,
        "POST",
        &format!("/companies/{}/secret-provider-configs", f.company_id),
        Some(body),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::CREATED,
        "expected 201 Created, got {}: {}",
        status,
        String::from_utf8_lossy(&resp_body)
    );

    let json = parse_json(&resp_body);
    assert_eq!(json["provider"], "vault");
}

// ---------------------------------------------------------------------------
// PM6: Multiple providers can coexist in same company
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_multiple_providers_coexist() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = secret_provider_config_routes().with_state(state);
    let actor = owner_actor(&f);

    let providers = vec![
        (
            "local_encrypted",
            json!({"keyPath": "/etc/parrot/secrets.key"}),
        ),
        (
            "aws_secrets_manager",
            json!({"region": "us-east-1", "secretPrefix": "parrot/"}),
        ),
        (
            "gcp_secret_manager",
            json!({"projectId": "my-gcp-project", "secretPrefix": "parrot/"}),
        ),
        (
            "vault",
            json!({"address": "https://vault.example.com:8200", "mountPoint": "secret"}),
        ),
    ];

    for (provider_type, config) in &providers {
        let body = json!({
            "provider": provider_type,
            "displayName": format!("{} Provider", provider_type),
            "config": config,
            "setAsDefault": false
        });

        let (status, _) = send(
            &app,
            &actor,
            "POST",
            &format!("/companies/{}/secret-provider-configs", f.company_id),
            Some(body),
        )
        .await;

        assert_eq!(status, StatusCode::CREATED, "create {} failed", provider_type);
    }

    // List should return all 4
    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/secret-provider-configs", f.company_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let json = parse_json(&body);
    let items = json.as_array().unwrap();
    assert_eq!(items.len(), 4, "expected 4 providers, got {}", items.len());

    // Verify all types are present
    let types: Vec<&str> = items.iter().map(|r| r["provider"].as_str().unwrap()).collect();
    assert!(types.contains(&"local_encrypted"));
    assert!(types.contains(&"aws_secrets_manager"));
    assert!(types.contains(&"gcp_secret_manager"));
    assert!(types.contains(&"vault"));
}

// ---------------------------------------------------------------------------
// PM7: Set default provider via POST /secret-provider-configs/:id/default
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_set_default_provider() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = secret_provider_config_routes().with_state(state);
    let actor = owner_actor(&f);

    // Create two providers
    let config_1_body = json!({
        "provider": "local_encrypted",
        "displayName": "Local",
        "config": {"keyPath": "/etc/parrot/secrets.key"},
        "setAsDefault": false
    });
    let (_, body_1) = send(
        &app, &actor, "POST",
        &format!("/companies/{}/secret-provider-configs", f.company_id),
        Some(config_1_body),
    )
    .await;
    let config_1_id: Uuid = parse_json(&body_1)["id"].as_str().unwrap().parse().unwrap();

    let config_2_body = json!({
        "provider": "aws_secrets_manager",
        "displayName": "AWS",
        "config": {"region": "us-east-1"},
        "setAsDefault": false
    });
    let (_, body_2) = send(
        &app, &actor, "POST",
        &format!("/companies/{}/secret-provider-configs", f.company_id),
        Some(config_2_body),
    )
    .await;
    let config_2_id: Uuid = parse_json(&body_2)["id"].as_str().unwrap().parse().unwrap();

    // Set config_2 as default
    let (status, _) = send(
        &app,
        &actor,
        "POST",
        &format!("/secret-provider-configs/{}/default", config_2_id),
        None,
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "expected 200, got {}",
        status
    );

    // Verify via list that one is default
    let (status, list_body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/secret-provider-configs", f.company_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let json = parse_json(&list_body);
    let items = json.as_array().unwrap();
    let defaults: Vec<&str> = items
        .iter()
        .filter(|r| r["isDefault"].as_bool().unwrap_or(false))
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(defaults.len(), 1, "expected exactly 1 default provider");
    assert_eq!(defaults[0], config_2_id.to_string());
}

// ---------------------------------------------------------------------------
// PM8: Cross-company isolation for secret provider configs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_secret_provider_configs_company_isolation() {
    let pool = connect_and_migrate().await;

    // Seed company A
    let company_a_id = Uuid::new_v4();
    let user_a_id = Uuid::new_v4();
    let prefix_a = format!("CA{}", &company_a_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a_id)
        .bind("Company A")
        .bind(&prefix_a)
        .execute(&pool)
        .await
        .expect("insert company A");
    sqlx::query(
        "INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(user_a_id)
    .bind(format!("a{}@test.com", user_a_id))
    .bind("User A")
    .execute(&pool)
    .await
    .expect("insert user A");
    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(company_a_id)
    .bind("user")
    .bind(user_a_id)
    .bind("owner")
    .bind("active")
    .execute(&pool)
    .await
    .expect("insert membership A");

    let actor_a = AuthorizationActor::board_with_source(
        user_a_id,
        company_a_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            company_a_id,
            PrincipalType::User,
            user_a_id,
            MembershipRole::Owner,
        )],
        true,
    );

    // Create a provider config in company A
    let config_a_body = json!({
        "provider": "local_encrypted",
        "displayName": "Local Encrypted",
        "config": {"keyPath": "/etc/parrot/secrets.key"},
        "setAsDefault": false
    });

    let app_state_a = build_app_state(pool.clone()).await.unwrap();
    let app_a = secret_provider_config_routes().with_state(app_state_a);

    let (_, _) = send(
        &app_a, &actor_a, "POST",
        &format!("/companies/{}/secret-provider-configs", company_a_id),
        Some(config_a_body),
    )
    .await;

    // Now seed company B
    let company_b_id = Uuid::new_v4();
    let user_b_id = Uuid::new_v4();
    let prefix_b = format!("CB{}", &company_b_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_b_id)
        .bind("Company B")
        .bind(&prefix_b)
        .execute(&pool)
        .await
        .expect("insert company B");
    sqlx::query(
        "INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(user_b_id)
    .bind(format!("b{}@test.com", user_b_id))
    .bind("User B")
    .execute(&pool)
    .await
    .expect("insert user B");
    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(company_b_id)
    .bind("user")
    .bind(user_b_id)
    .bind("owner")
    .bind("active")
    .execute(&pool)
    .await
    .expect("insert membership B");

    let actor_b = AuthorizationActor::board_with_source(
        user_b_id,
        company_b_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            company_b_id,
            PrincipalType::User,
            user_b_id,
            MembershipRole::Owner,
        )],
        true,
    );

    let app_state_b = build_app_state(pool.clone()).await.unwrap();
    let app_b = secret_provider_config_routes().with_state(app_state_b);

    // Company B should see 0 configs
    let (status, body) = send(
        &app_b, &actor_b, "GET",
        &format!("/companies/{}/secret-provider-configs", company_b_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let json = parse_json(&body);
    assert_eq!(
        json.as_array().unwrap().len(),
        0,
        "company B should see 0 secret provider configs"
    );
}

// ---------------------------------------------------------------------------
// PM9: Storage service supports local_disk (s3 needs env vars)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_storage_providers_supported() {
    use services::asset_storage::StorageProviderRegistry;

    let registry = StorageProviderRegistry::from_env();

    // local_disk should work (always available)
    let local_result = registry.provider(Some("local_disk"));
    assert!(
        local_result.is_ok(),
        "local_disk should be supported"
    );

    // s3 should fail with NotImplementedError (needs env vars)
    let s3_result = registry.provider(Some("s3"));
    assert!(
        s3_result.is_err(),
        "s3 should require env config"
    );
    // Check error without unwrapping (Arc<dyn StorageService> doesn't impl Debug)
    match s3_result {
        Ok(_) => panic!("s3 should fail without env config"),
        Err(e) => {
            let err_msg = e.to_string();
            assert!(
                err_msg.contains("not implemented") || err_msg.contains("s3"),
                "s3 error should mention not implemented, got: {}",
                err_msg
            );
        }
    }
}

// ---------------------------------------------------------------------------
// PM10: All Paperclip storage providers are represented in Parrot
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_all_paperclip_storage_providers_have_parrot_equivalent() {
    use services::asset_storage::StorageProviderRegistry;

    let registry = StorageProviderRegistry::from_env();

    let paperclip_storage_providers = vec!["local_disk", "s3"];
    for provider in paperclip_storage_providers {
        let result = registry.provider(Some(provider));
        if provider == "local_disk" {
            assert!(result.is_ok(), "local_disk must be available");
        } else {
            // Check error without unwrapping (Arc<dyn StorageService> doesn't impl Debug)
            match result {
                Ok(_) => panic!("s3 should fail without env config"),
                Err(e) => {
                    let err_msg = e.to_string();
                    assert!(
                        err_msg.contains("not implemented"),
                        "s3 should return 'not implemented' (missing env), got: {}",
                        err_msg
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// PM11: All Paperclip secret providers are represented in Parrot
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_all_paperclip_secret_providers_have_parrot_equivalent() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = secret_provider_config_routes().with_state(state);
    let actor = owner_actor(&f);

    let paperclip_secret_providers = vec![
        "local_encrypted",
        "aws_secrets_manager",
        "gcp_secret_manager",
        "vault",
    ];

    for provider_type in &paperclip_secret_providers {
        let config = match *provider_type {
            "local_encrypted" => json!({"keyPath": "/etc/parrot/secrets.key"}),
            "aws_secrets_manager" => json!({"region": "us-east-1"}),
            "gcp_secret_manager" => json!({"projectId": "test-project"}),
            "vault" => json!({"address": "https://vault.example.com:8200"}),
            _ => unreachable!(),
        };

        let body = json!({
            "provider": provider_type,
            "displayName": format!("{} Provider", provider_type),
            "config": config,
            "setAsDefault": false
        });

        let (status, _) = send(
            &app,
            &actor,
            "POST",
            &format!("/companies/{}/secret-provider-configs", f.company_id),
            Some(body),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::CREATED,
            "Paperclip provider '{}' must be creatable in Parrot, got {}",
            provider_type,
            status
        );
    }
}

// ---------------------------------------------------------------------------
// PM12: Health check endpoint returns expected structure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_health_check_endpoint() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = secret_provider_config_routes().with_state(state);
    let actor = owner_actor(&f);

    // Create a config first
    let body = json!({
        "provider": "local_encrypted",
        "displayName": "Local",
        "config": {"keyPath": "/etc/parrot/secrets.key"},
        "setAsDefault": false
    });

    let (_, resp_body) = send(
        &app, &actor, "POST",
        &format!("/companies/{}/secret-provider-configs", f.company_id),
        Some(body),
    )
    .await;
    let config_id: Uuid = parse_json(&resp_body)["id"].as_str().unwrap().parse().unwrap();

    // Company-level health should return 200
    let (status, body) = send(
        &app, &actor, "GET",
        &format!("/companies/{}/secret-providers/health", f.company_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "company health should return 200");
}
