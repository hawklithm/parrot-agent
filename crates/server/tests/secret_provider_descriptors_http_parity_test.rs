//! HTTP parity tests for `GET /companies/:companyId/secret-providers` (SE5).
//!
//! Reference contract: `paperclip/server/src/secrets/provider-registry.ts:28`
//! (`listSecretProviders()`) plus each provider module's `descriptor()`.
//!
//! This route lives in `api::routes::secrets::secret_routes()`, which the
//! provider-config matrix test does not mount, so it is covered here against
//! the full application router.
//!
//! Run with a live database:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/parrot_agent_compile \
//!     cargo test -p parrot-server --test secret_provider_descriptors_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::secrets::secret_routes;
use parrot_server::build_app_state;
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};

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
    let prefix = format!("SD{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("SecretDescriptorTest")
        .bind(&prefix)
        .execute(pool)
        .await
        .expect("insert company");

    sqlx::query(
        "INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(format!("sd{}@test.com", user_id))
    .bind("SD User")
    .execute(pool)
    .await
    .expect("insert auth_user");

    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, \
         membership_role, status) VALUES ($1, $2::principal_type, $3, \
         $4::membership_role, $5::company_membership_status) ON CONFLICT DO NOTHING",
    )
    .bind(company_id)
    .bind("user")
    .bind(user_id)
    .bind("owner")
    .bind("active")
    .execute(pool)
    .await
    .expect("insert membership");

    Fixture { company_id, user_id }
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
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    let mut req = builder
        .body(Body::empty())
        .expect("build request");
    req.extensions_mut().insert(actor.clone());
    let resp = app.clone().oneshot(req).await.expect("dispatch request");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    (status, bytes.to_vec())
}

/// Same as `send`, plus a JSON body — needed to exercise a *write* route's
/// authorization gate rather than only its read path.
async fn send_json(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
    body: &Value,
) -> (StatusCode, Vec<u8>) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(body).expect("serialize request body"),
        ))
        .expect("build request");
    let mut req = req;
    req.extensions_mut().insert(actor.clone());
    let resp = app.clone().oneshot(req).await.expect("dispatch request");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    (status, bytes.to_vec())
}

fn parse(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).expect("response body must be JSON")
}

/// Mount the secrets router directly.
///
/// The full app router installs the auth middleware, which resolves the actor
/// from request headers and overwrites whatever the test injected — so a test
/// could not choose its own tenant. Mounting the router in isolation keeps the
/// injected actor authoritative, which is how the sibling route tests work.
async fn app_for(pool: &PgPool) -> Router {
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    secret_routes().with_state(state)
}

// ---------------------------------------------------------------------------
// SD1: response is a flat array of four descriptors in registry order
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sd1_descriptors_are_registry_ordered() {
    let pool = connect_and_migrate().await;
    let fixture = seed(&pool).await;
    let app = app_for(&pool).await;
    let actor = owner_actor(&fixture);

    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/secret-providers", fixture.company_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));

    let descriptors = parse(&body);
    let list = descriptors
        .as_array()
        .unwrap_or_else(|| panic!("descriptors must be a flat array, got: {descriptors}"));
    assert_eq!(list.len(), 4, "registry has four providers: {descriptors}");

    let ids: Vec<&str> = list
        .iter()
        .map(|d| d["id"].as_str().expect("id must be a string"))
        .collect();
    assert_eq!(
        ids,
        vec![
            "local_encrypted",
            "aws_secrets_manager",
            "gcp_secret_manager",
            "vault"
        ],
        "order must match Paperclip's provider-registry.ts"
    );
}

// ---------------------------------------------------------------------------
// SD2: per-provider capability flags
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sd2_capability_flags_match_provider_modules() {
    let pool = connect_and_migrate().await;
    let fixture = seed(&pool).await;
    let app = app_for(&pool).await;
    let actor = owner_actor(&fixture);

    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/secret-providers", fixture.company_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let list = parse(&body);
    let find = |id: &str| -> Value {
        list.as_array()
            .expect("array")
            .iter()
            .find(|d| d["id"] == id)
            .unwrap_or_else(|| panic!("{id} descriptor missing"))
            .clone()
    };

    // local_encrypted: always configured, managed values only.
    let local = find("local_encrypted");
    assert_eq!(local["configured"], true, "{local}");
    assert_eq!(local["requiresExternalRef"], false, "{local}");
    assert_eq!(local["supportsManagedValues"], true, "{local}");
    assert_eq!(local["supportsExternalReferences"], false, "{local}");
    assert_eq!(local["label"], "Local encrypted (default)", "{local}");

    // aws_secrets_manager: managed values AND external references/values.
    let aws = find("aws_secrets_manager");
    assert_eq!(aws["requiresExternalRef"], false, "{aws}");
    assert_eq!(aws["supportsManagedValues"], true, "{aws}");
    assert_eq!(aws["supportsExternalReferences"], true, "{aws}");
    assert_eq!(aws["supportsExternalValueWrites"], true, "{aws}");
    assert_eq!(aws["label"], "AWS Secrets Manager", "{aws}");

    // The stub providers declare one shape and nothing else.
    for id in ["gcp_secret_manager", "vault"] {
        let stub = find(id);
        assert_eq!(stub["requiresExternalRef"], true, "{stub}");
        assert_eq!(stub["supportsManagedValues"], false, "{stub}");
        assert_eq!(stub["supportsExternalReferences"], true, "{stub}");
        assert_eq!(stub["supportsExternalValueWrites"], false, "{stub}");
        assert_eq!(stub["configured"], false, "{stub}");
    }
    assert_eq!(find("gcp_secret_manager")["label"], "GCP Secret Manager");
    assert_eq!(find("vault")["label"], "HashiCorp Vault");
}

// ---------------------------------------------------------------------------
// SD3: `configured` is deployment readiness, not per-company state
// ---------------------------------------------------------------------------

/// A company with a stored, ready AWS config must NOT change the descriptor:
/// Paperclip derives `configured` from `getAwsConfigReadiness()`. The old
/// implementation queried `status = 'active'`, a value no row can hold, so the
/// predicate was permanently false.
#[tokio::test]
async fn sd3_configured_is_deployment_scoped() {
    let pool = connect_and_migrate().await;
    let fixture = seed(&pool).await;
    let app = app_for(&pool).await;
    let actor = owner_actor(&fixture);

    let config_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO company_secret_provider_configs \
         (id, company_id, provider, display_name, status, is_default, config) \
         VALUES ($1, $2, 'aws_secrets_manager', 'AWS', 'ready', false, \
                 '{\"region\":\"us-east-1\"}'::jsonb)",
    )
    .bind(config_id)
    .bind(fixture.company_id)
    .execute(&pool)
    .await
    .expect("insert aws config");

    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/secret-providers", fixture.company_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));

    let aws = parse(&body)
        .as_array()
        .expect("array")
        .iter()
        .find(|d| d["id"] == "aws_secrets_manager")
        .expect("aws descriptor")
        .clone();

    // The stored row must not leak into the descriptor's `configured`.
    let expected = ["PARROT_AWS_REGION", "AWS_REGION", "AWS_DEFAULT_REGION"]
        .iter()
        .any(|name| std::env::var(name).is_ok_and(|v| !v.trim().is_empty()))
        && std::env::var("PARROT_SECRETS_AWS_DEPLOYMENT_ID").is_ok()
        && std::env::var("PARROT_SECRETS_AWS_KMS_KEY_ID").is_ok();
    assert_eq!(
        aws["configured"], expected,
        "`configured` must reflect deployment env readiness only, not a company row: {aws}"
    );
}

// ---------------------------------------------------------------------------
// SD4: cross-company access to the descriptor list is 403
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sd4_cross_company_descriptor_list_is_forbidden() {
    let pool = connect_and_migrate().await;
    let company_a = seed(&pool).await;
    let company_b = seed(&pool).await;
    let app = app_for(&pool).await;

    // Actor A is only a member of company A.
    let actor_a = owner_actor(&company_a);
    let (status, body) = send(
        &app,
        &actor_a,
        "GET",
        &format!("/companies/{}/secret-providers", company_b.company_id),
    )
    .await;

    // A company path param is an address, not a resource id, so Paperclip's
    // `assertCompanyAccess` raises 403 rather than hiding behind a 404.
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "{}",
        String::from_utf8_lossy(&body)
    );
}

// ---------------------------------------------------------------------------
// SD5: every company-scoped secret route denies a non-member with 403
// ---------------------------------------------------------------------------

/// Paperclip gates all four company-scoped secret routes with
/// `assertCompanyAccess`, whose denial is `forbidden` — 403, not 404. Only
/// *resource-id* lookups hide behind 404 (§18.1). A regression to `NotFound`
/// would still "look" like a denial while leaking the company's existence.
#[tokio::test]
async fn sd5_company_scoped_secret_routes_forbid_non_members() {
    let pool = connect_and_migrate().await;
    let company_a = seed(&pool).await;
    let company_b = seed(&pool).await;
    let app = app_for(&pool).await;
    let actor_a = owner_actor(&company_a);

    let foreign = company_b.company_id;
    let create_body = json!({
        "name": "FORBIDDEN_PROBE",
        "key": "FORBIDDEN_PROBE",
        "value": "irrelevant",
    });

    let cases: [(&str, &str, Option<Value>); 4] = [
        ("GET", "list company secrets", None),
        ("POST", "create company secret", Some(create_body)),
        ("GET", "read catalog", None),
        ("GET", "provider descriptors", None),
    ];
    let uris = [
        format!("/companies/{foreign}/secrets"),
        format!("/companies/{foreign}/secrets"),
        format!("/companies/{foreign}/secrets/catalog"),
        format!("/companies/{foreign}/secret-providers"),
    ];

    for ((method, label, body), uri) in cases.into_iter().zip(uris) {
        let (status, bytes) = match body {
            Some(value) => send_json(&app, &actor_a, method, &uri, &value).await,
            None => send(&app, &actor_a, method, &uri).await,
        };
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{label} cross-company must be 403, got {}: {}",
            status,
            String::from_utf8_lossy(&bytes)
        );
    }

    // The rejection must be an authorization decision, not a side effect:
    // nothing was written into company B.
    let leaked: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM company_secrets WHERE company_id = $1")
            .bind(company_b.company_id)
            .fetch_one(&pool)
            .await
            .expect("count company B secrets");
    assert_eq!(leaked, 0, "a forbidden write must not create a row");
}
