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
use services::errors::ServiceError;

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------


mod common;
use common::connect_and_migrate;


struct Fixture {
    company_id: Uuid,
    user_id: Uuid,
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
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, $2::principal_type, $3, $4::membership_role, $5::company_membership_status) ON CONFLICT DO NOTHING",
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
        company_id,
        user_id,
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

/// Create a provider vault and return its id, asserting the documented `201`.
async fn create_config(
    app: &Router,
    actor: &AuthorizationActor,
    company_id: Uuid,
    body: Value,
) -> Uuid {
    let (status, resp_body) = send(
        app,
        actor,
        "POST",
        &format!("/companies/{company_id}/secret-provider-configs"),
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
    parse_json(&resp_body)["id"]
        .as_str()
        .expect("created config id must be a string")
        .parse()
        .expect("created config id must be a UUID")
}

/// Assert a response status, printing the body verbatim on mismatch.
fn assert_status(actual: StatusCode, expected: StatusCode, label: &str, body: &[u8]) {
    assert_eq!(
        actual,
        expected,
        "{label}: expected {expected}, got {actual}: {}",
        String::from_utf8_lossy(body)
    );
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

    // `local_encrypted.config` is `.strict()` and accepts only a single
    // optional boolean, so there is no meaningful config to send.
    let body = json!({
        "provider": "local_encrypted",
        "displayName": "Local Encrypted Store",
        "config": {},
        "isDefault": false
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
    // A non-coming-soon provider defaults to `ready`.
    assert_eq!(json["status"], "ready");
    assert_eq!(json["isDefault"], false);
    assert_eq!(json["companyId"], f.company_id.to_string());

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
            "secretNamePrefix": "parrot/"
        },
        "isDefault": false
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
    assert_eq!(json["status"], "ready");
    assert_eq!(json["config"]["region"], "us-east-1");
    assert_eq!(json["config"]["secretNamePrefix"], "parrot/");
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
            "secretNamePrefix": "parrot/"
        },
        "isDefault": false
    });

    let (status, resp_body) = send(
        &app,
        &actor,
        "POST",
        &format!("/companies/{}/secret-provider-configs", f.company_id),
        Some(body),
    )
    .await;

    // Creating a `coming_soon` draft is explicitly allowed: only a *live*
    // status is rejected.
    assert_eq!(
        status,
        StatusCode::CREATED,
        "expected 201 Created, got {}: {}",
        status,
        String::from_utf8_lossy(&resp_body)
    );

    let json = parse_json(&resp_body);
    assert_eq!(json["provider"], "gcp_secret_manager");
    assert_eq!(
        json["status"], "coming_soon",
        "gcp_secret_manager defaults to coming_soon, got: {json}"
    );
    assert_eq!(json["config"]["projectId"], "my-gcp-project");
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
            "mountPath": "secret"
        },
        "isDefault": false
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
    assert_eq!(
        json["status"], "coming_soon",
        "vault defaults to coming_soon, got: {json}"
    );
    // `vaultAddressSchema` normalizes an origin-only URL to its origin.
    assert_eq!(json["config"]["address"], "https://vault.example.com:8200");
    assert_eq!(json["config"]["mountPath"], "secret");
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
        ("local_encrypted", json!({})),
        (
            "aws_secrets_manager",
            json!({"region": "us-east-1", "secretNamePrefix": "parrot/"}),
        ),
        (
            "gcp_secret_manager",
            json!({"projectId": "my-gcp-project", "secretNamePrefix": "parrot/"}),
        ),
        (
            "vault",
            json!({"address": "https://vault.example.com:8200", "mountPath": "secret"}),
        ),
    ];

    for (provider_type, config) in &providers {
        let body = json!({
            "provider": provider_type,
            "displayName": format!("{} Provider", provider_type),
            "config": config,
            "isDefault": false
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
            "create {} failed, got {}: {}",
            provider_type,
            status,
            String::from_utf8_lossy(&resp_body)
        );
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

    // Only the non-coming-soon providers are live.
    for row in items {
        let expected = match row["provider"].as_str().unwrap() {
            "gcp_secret_manager" | "vault" => "coming_soon",
            _ => "ready",
        };
        assert_eq!(
            row["status"], expected,
            "unexpected default status for {}: {}",
            row["provider"], row
        );
    }
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
        "config": {},
        "isDefault": false
    });
    let config_1_id = create_config(&app, &actor, f.company_id, config_1_body).await;

    let config_2_body = json!({
        "provider": "aws_secrets_manager",
        "displayName": "AWS",
        "config": {"region": "us-east-1"},
        "isDefault": false
    });
    let config_2_id = create_config(&app, &actor, f.company_id, config_2_body).await;

    // Set config_2 as default
    let (status, body) = send(
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
        "expected 200, got {}: {}",
        status,
        String::from_utf8_lossy(&body)
    );
    let promoted = parse_json(&body);
    assert_eq!(promoted["id"], config_2_id.to_string());
    assert_eq!(promoted["isDefault"], true);

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
    let _ = config_1_id;
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
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, $2::principal_type, $3, $4::membership_role, $5::company_membership_status) ON CONFLICT DO NOTHING",
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

    let app_state_a = build_app_state(pool.clone()).await.unwrap();
    let app_a = secret_provider_config_routes().with_state(app_state_a);

    // Create a provider config in company A
    let config_a_body = json!({
        "provider": "local_encrypted",
        "displayName": "Local Encrypted",
        "config": {},
        "isDefault": false
    });
    let config_a_id = create_config(&app_a, &actor_a, company_a_id, config_a_body).await;

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
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, $2::principal_type, $3, $4::membership_role, $5::company_membership_status) ON CONFLICT DO NOTHING",
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

    // Addressing company A's row *by id* is a 404 for B, not a 403: the
    // resource is the address, so a miss and a foreign row are indistinguishable.
    let (status, body) = send(
        &app_b,
        &actor_b,
        "GET",
        &format!("/secret-provider-configs/{config_a_id}"),
        None,
    )
    .await;
    assert_status(status, StatusCode::NOT_FOUND, "foreign GET by id", &body);
    assert_eq!(parse_json(&body)["error"], "Provider vault not found");

    // Address by id under company B's own path is a 403 (company is the address).
    let (status, body) = send(
        &app_b,
        &actor_b,
        "GET",
        &format!("/companies/{}/secret-provider-configs", company_a_id),
        None,
    )
    .await;
    assert_status(
        status,
        StatusCode::FORBIDDEN,
        "foreign company-scoped list",
        &body,
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
    assert!(local_result.is_ok(), "local_disk should be supported");

    // s3 without credentials must fail with the typed NotImplemented error.
    // `ServiceError::NotImplemented` Display is "Not implemented: {0}" — capital
    // N, so match the variant instead of lowercasing the message.
    let s3_result = registry.provider(Some("s3"));
    assert!(
        matches!(s3_result, Err(ServiceError::NotImplemented(_))),
        "s3 without env config must be NotImplemented, got: {:?}",
        s3_result.map(|_| "Ok(Arc<dyn StorageService>)").err()
    );
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
            assert!(
                matches!(result, Err(ServiceError::NotImplemented(_))),
                "s3 without env config must be NotImplemented"
            );
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
            "local_encrypted" => json!({}),
            "aws_secrets_manager" => json!({"region": "us-east-1"}),
            "gcp_secret_manager" => json!({"projectId": "test-project"}),
            "vault" => json!({"address": "https://vault.example.com:8200"}),
            _ => unreachable!(),
        };

        let body = json!({
            "provider": provider_type,
            "displayName": format!("{} Provider", provider_type),
            "config": config,
            "isDefault": false
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
            "Paperclip provider '{}' must be creatable in Parrot, got {}: {}",
            provider_type,
            status,
            String::from_utf8_lossy(&resp_body)
        );

        let created = parse_json(&resp_body);
        let expected_status = if matches!(
            *provider_type,
            "gcp_secret_manager" | "vault"
        ) {
            "coming_soon"
        } else {
            "ready"
        };
        assert_eq!(
            created["status"], expected_status,
            "unexpected default status for {provider_type}: {created}"
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
        "config": {},
        "isDefault": false
    });
    let config_id = create_config(&app, &actor, f.company_id, body).await;

    // Company-level health should return 200 with the registry-shaped body.
    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/secret-providers/health", f.company_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "company health should return 200");
    let json = parse_json(&body);
    let providers = json["providers"]
        .as_array()
        .expect("health body must carry a providers array");
    assert_eq!(providers.len(), 4, "registry must report 4 providers");

    // Per-config health of a `ready` local_encrypted vault probes the module.
    let (status, health_body) = send(
        &app,
        &actor,
        "POST",
        &format!("/secret-provider-configs/{config_id}/health"),
        None,
    )
    .await;
    assert_status(status, StatusCode::OK, "per-config health", &health_body);
    let health = parse_json(&health_body);
    assert_eq!(health["configId"], config_id.to_string());
    assert_eq!(health["provider"], "local_encrypted");
    assert!(
        health["status"].is_string(),
        "health status must be a string: {health}"
    );
}

// ---------------------------------------------------------------------------
// PM13: Cross-tenant by-id access is a 404, never a 403 or a 200
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_cross_tenant_by_id_is_404() {
    let pool = connect_and_migrate().await;

    // Company A (the owner of the row) and company B (the attacker).
    let f_a = seed(&pool).await;
    let f_b = seed(&pool).await;
    let actor_a = owner_actor(&f_a);
    let actor_b = owner_actor(&f_b);

    let state = build_app_state(pool.clone()).await.unwrap();
    let app = secret_provider_config_routes().with_state(state);

    let a_id = create_config(
        &app,
        &actor_a,
        f_a.company_id,
        json!({
            "provider": "local_encrypted",
            "displayName": "A Local",
            "config": {},
            "isDefault": false
        }),
    )
    .await;

    // Every by-id verb must be an indistinguishable 404 for actor B.
    let null_body: Option<Value> = None;
    let by_id_cases: Vec<(&str, String, Option<Value>)> = vec![
        (
            "GET",
            format!("/secret-provider-configs/{a_id}"),
            null_body.clone(),
        ),
        (
            "PATCH",
            format!("/secret-provider-configs/{a_id}"),
            Some(json!({"displayName": "stolen"})),
        ),
        ("DELETE", format!("/secret-provider-configs/{a_id}"), null_body.clone()),
        (
            "POST",
            format!("/secret-provider-configs/{a_id}/default"),
            null_body.clone(),
        ),
        (
            "POST",
            format!("/secret-provider-configs/{a_id}/health"),
            null_body.clone(),
        ),
    ];

    for (method, uri, body) in by_id_cases {
        let (status, resp_body) = send(&app, &actor_b, method, &uri, body).await;
        assert_status(
            status,
            StatusCode::NOT_FOUND,
            &format!("cross-tenant {method} {uri}"),
            &resp_body,
        );
        assert_eq!(
            parse_json(&resp_body)["error"],
            "Provider vault not found",
            "cross-tenant {method} must not disclose the row"
        );
    }

    // No leakage into B's own list.
    let (status, body) = send(
        &app,
        &actor_b,
        "GET",
        &format!("/companies/{}/secret-provider-configs", f_b.company_id),
        null_body.clone(),
    )
    .await;
    assert_status(status, StatusCode::OK, "company B list", &body);
    let b_items = parse_json(&body);
    assert_eq!(
        b_items.as_array().unwrap().len(),
        0,
        "company B must not see company A's config: {b_items}"
    );

    // The row survived every rejected verb and is still readable by its owner.
    let (status, body) = send(
        &app,
        &actor_a,
        "GET",
        &format!("/secret-provider-configs/{a_id}"),
        None,
    )
    .await;
    assert_status(status, StatusCode::OK, "owner GET after attacks", &body);
    let owned = parse_json(&body);
    assert_eq!(owned["id"], a_id.to_string());
    assert_eq!(
        owned["displayName"], "A Local",
        "owner's row must be unchanged: {owned}"
    );
    assert_eq!(owned["isDefault"], false, "default flag must be untouched");
}

// ---------------------------------------------------------------------------
// PM14: DELETE responds 200 with the removed row (not 204)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_delete_returns_removed_row() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = secret_provider_config_routes().with_state(state);
    let actor = owner_actor(&f);

    let config_id = create_config(
        &app,
        &actor,
        f.company_id,
        json!({
            "provider": "aws_secrets_manager",
            "displayName": "Doomed AWS",
            "config": {"region": "us-west-2"},
            "isDefault": false
        }),
    )
    .await;

    let (status, body) = send(
        &app,
        &actor,
        "DELETE",
        &format!("/secret-provider-configs/{config_id}"),
        None,
    )
    .await;

    assert_status(
        status,
        StatusCode::OK,
        "DELETE must be 200 with the removed row, not 204",
        &body,
    );
    let removed = parse_json(&body);
    assert_eq!(removed["id"], config_id.to_string());
    assert_eq!(removed["provider"], "aws_secrets_manager");
    assert_eq!(removed["displayName"], "Doomed AWS");
    assert_eq!(removed["companyId"], f.company_id.to_string());

    // The row is really gone.
    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/secret-provider-configs/{config_id}"),
        None,
    )
    .await;
    assert_status(status, StatusCode::NOT_FOUND, "GET after DELETE", &body);
    assert_eq!(parse_json(&body)["error"], "Provider vault not found");

    // And it no longer appears in the company list.
    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/secret-provider-configs", f.company_id),
        None,
    )
    .await;
    assert_status(status, StatusCode::OK, "list after DELETE", &body);
    assert_eq!(parse_json(&body).as_array().unwrap().len(), 0);
}

// ---------------------------------------------------------------------------
// PM15: Per-config health states
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_config_health_states() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = secret_provider_config_routes().with_state(state);
    let actor = owner_actor(&f);

    // (a) A coming_soon provider's runtime is locked.
    let gcp_id = create_config(
        &app,
        &actor,
        f.company_id,
        json!({
            "provider": "gcp_secret_manager",
            "displayName": "GCP Draft",
            "config": {"projectId": "test-project"},
            "isDefault": false
        }),
    )
    .await;

    let (status, body) = send(
        &app,
        &actor,
        "POST",
        &format!("/secret-provider-configs/{gcp_id}/health"),
        None,
    )
    .await;
    assert_status(status, StatusCode::OK, "coming_soon health", &body);
    let health = parse_json(&body);
    assert_eq!(health["configId"], gcp_id.to_string(), "configId must echo: {health}");
    assert_eq!(health["provider"], "gcp_secret_manager");
    assert_eq!(health["status"], "coming_soon", "body: {health}");
    assert_eq!(health["details"]["code"], "runtime_locked", "body: {health}");

    // (b) A local_encrypted vault patched to `disabled` reports `disabled`,
    // and the same PATCH clears the default flag.
    let local_id = create_config(
        &app,
        &actor,
        f.company_id,
        json!({
            "provider": "local_encrypted",
            "displayName": "Local Default",
            "config": {},
            "isDefault": true
        }),
    )
    .await;

    let (status, body) = send(
        &app,
        &actor,
        "PATCH",
        &format!("/secret-provider-configs/{local_id}"),
        Some(json!({"status": "disabled"})),
    )
    .await;
    assert_status(status, StatusCode::OK, "PATCH to disabled", &body);
    let patched = parse_json(&body);
    assert_eq!(patched["status"], "disabled", "body: {patched}");
    assert_eq!(
        patched["isDefault"], false,
        "disabling a vault must clear isDefault: {patched}"
    );

    let (status, body) = send(
        &app,
        &actor,
        "POST",
        &format!("/secret-provider-configs/{local_id}/health"),
        None,
    )
    .await;
    assert_status(status, StatusCode::OK, "disabled health", &body);
    let health = parse_json(&body);
    assert_eq!(health["configId"], local_id.to_string());
    assert_eq!(health["status"], "disabled", "body: {health}");
    assert_eq!(health["details"]["code"], "disabled", "body: {health}");
}

// ---------------------------------------------------------------------------
// PM16: Company health wrapper shape
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_company_health_wrapper_shape() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = secret_provider_config_routes().with_state(state);
    let actor = owner_actor(&f);

    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/secret-providers/health", f.company_id),
        None,
    )
    .await;
    assert_status(status, StatusCode::OK, "company health", &body);

    let json = parse_json(&body);
    assert!(
        json.is_object(),
        "health body must be an object wrapper, got: {json}"
    );
    let providers = json["providers"]
        .as_array()
        .unwrap_or_else(|| panic!("body must carry a providers array: {json}"));
    assert_eq!(providers.len(), 4, "expected exactly 4 providers: {json}");

    let ids: Vec<&str> = providers
        .iter()
        .map(|p| {
            p["provider"]
                .as_str()
                .unwrap_or_else(|| panic!("every check needs a provider id: {p}"))
        })
        .collect();
    assert_eq!(
        ids,
        vec![
            "local_encrypted",
            "aws_secrets_manager",
            "gcp_secret_manager",
            "vault"
        ],
        "providers must appear in registry order: {json}"
    );

    for check in providers {
        let status = check["status"]
            .as_str()
            .unwrap_or_else(|| panic!("every check needs a status: {check}"));
        assert!(
            matches!(status, "ok" | "warn" | "error"),
            "health status must be ok|warn|error, got {status}: {check}"
        );
        let message = check["message"]
            .as_str()
            .unwrap_or_else(|| panic!("every check needs a message: {check}"));
        assert!(
            !message.trim().is_empty(),
            "health message must not be empty: {check}"
        );
    }
}

// ---------------------------------------------------------------------------
// PM17: The 400 / 422 split
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_validation_status_code_split() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = secret_provider_config_routes().with_state(state);
    let actor = owner_actor(&f);
    let create_uri = format!("/companies/{}/secret-provider-configs", f.company_id);

    // Each case is a request-shape violation, rejected before any state is read.
    let bad_creates: Vec<(&str, Value)> = vec![
        (
            "sensitive config key",
            json!({
                "provider": "local_encrypted",
                "displayName": "Sensitive",
                "config": {"token": "x"}
            }),
        ),
        (
            "unknown config key",
            json!({
                "provider": "local_encrypted",
                "displayName": "Unknown Key",
                "config": {"nope": 1}
            }),
        ),
        (
            "malformed aws region",
            json!({
                "provider": "aws_secrets_manager",
                "displayName": "Bad Region",
                "config": {"region": "us_gov_east"}
            }),
        ),
        (
            "gcp forced live",
            json!({
                "provider": "gcp_secret_manager",
                "displayName": "GCP Live",
                "status": "ready",
                "config": {"projectId": "test-project"}
            }),
        ),
        (
            "disabled default",
            json!({
                "provider": "local_encrypted",
                "displayName": "Disabled Default",
                "status": "disabled",
                "isDefault": true,
                "config": {}
            }),
        ),
        (
            "blank displayName",
            json!({
                "provider": "local_encrypted",
                "displayName": "   ",
                "config": {}
            }),
        ),
        (
            "unknown provider",
            json!({
                "provider": "nope",
                "displayName": "Unknown Provider",
                "config": {}
            }),
        ),
        (
            "missing aws region",
            json!({
                "provider": "aws_secrets_manager",
                "displayName": "No Region",
                "config": {}
            }),
        ),
    ];

    for (label, body) in bad_creates {
        let (status, resp_body) = send(&app, &actor, "POST", &create_uri, Some(body)).await;
        assert_status(
            status,
            StatusCode::BAD_REQUEST,
            &format!("POST must be 400 for {label}"),
            &resp_body,
        );
    }

    // (i) `POST /:id/default` on a coming_soon gcp draft: semantically understood,
    // refused by the service.
    let gcp_id = create_config(
        &app,
        &actor,
        f.company_id,
        json!({
            "provider": "gcp_secret_manager",
            "displayName": "GCP Draft",
            "config": {"projectId": "test-project"}
        }),
    )
    .await;

    let (status, body) = send(
        &app,
        &actor,
        "POST",
        &format!("/secret-provider-configs/{gcp_id}/default"),
        None,
    )
    .await;
    assert_status(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "defaulting a coming_soon vault must be 422",
        &body,
    );

    // (j) PATCH that would promote a gcp draft out of coming_soon → 422.
    let (status, body) = send(
        &app,
        &actor,
        "PATCH",
        &format!("/secret-provider-configs/{gcp_id}"),
        Some(json!({"status": "ready"})),
    )
    .await;
    assert_status(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "promoting a gcp vault via PATCH must be 422",
        &body,
    );

    // (k) PATCH's narrow pre-check does not inspect unknown keys, so the same
    // key that is a 400 on POST reaches the service and surfaces as 422.
    let local_id = create_config(
        &app,
        &actor,
        f.company_id,
        json!({
            "provider": "local_encrypted",
            "displayName": "Local"
        }),
    )
    .await;
    let (status, body) = send(
        &app,
        &actor,
        "PATCH",
        &format!("/secret-provider-configs/{local_id}"),
        Some(json!({"config": {"nope": 1}})),
    )
    .await;
    assert_status(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "PATCH with an unknown config key must be 422",
        &body,
    );
}

// ---------------------------------------------------------------------------
// PM18: An absent `config` means `{}`, not a rejection
// ---------------------------------------------------------------------------

/// Paperclip's create schema is `config: providerConfigSchema.default({})`, so a
/// payload that omits `config` entirely is valid and stores `{}`. This guards the
/// regression where `serde_json::Value` defaulted to `Null` and the service
/// rejected the non-object as `400 Invalid provider vault config`.
#[tokio::test]
async fn test_absent_config_defaults_to_empty_object() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = secret_provider_config_routes().with_state(state);
    let actor = owner_actor(&f);

    let (status, body) = send(
        &app,
        &actor,
        "POST",
        &format!("/companies/{}/secret-provider-configs", f.company_id),
        Some(json!({
            "provider": "local_encrypted",
            "displayName": "No Config Field"
        })),
    )
    .await;
    assert_status(
        status,
        StatusCode::CREATED,
        "an omitted config must default to an empty object",
        &body,
    );
    let created = parse_json(&body);
    assert_eq!(created["config"], json!({}), "body: {created}");
    assert_eq!(created["status"], "ready", "body: {created}");
    assert_eq!(created["isDefault"], false, "body: {created}");

    // A PATCH without `config` leaves the stored object untouched.
    let config_id = created["id"].as_str().expect("created id").to_string();
    let (status, body) = send(
        &app,
        &actor,
        "PATCH",
        &format!("/secret-provider-configs/{config_id}"),
        Some(json!({"displayName": "Renamed"})),
    )
    .await;
    assert_status(status, StatusCode::OK, "rename without config", &body);
    let patched = parse_json(&body);
    assert_eq!(patched["config"], json!({}), "body: {patched}");
    assert_eq!(patched["displayName"], "Renamed", "body: {patched}");

    // `.default({})` fires only on an ABSENT key: an explicit `null` is still a
    // shape violation, which is what makes the assertion above meaningful.
    let (status, body) = send(
        &app,
        &actor,
        "POST",
        &format!("/companies/{}/secret-provider-configs", f.company_id),
        Some(json!({
            "provider": "local_encrypted",
            "displayName": "Null Config",
            "config": null
        })),
    )
    .await;
    assert_status(
        status,
        StatusCode::BAD_REQUEST,
        "an explicit null config must still be rejected",
        &body,
    );
}
