//! HTTP parity tests for instance database backup/restore routes.
//!
//! These tests stand up the real Axum router with a real AppState and inject
//! AuthorizationActor extensions, bypassing the global auth middleware.

use api::routes::instance_settings::instance_settings_routes;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use parrot_server::build_app_state;
use serde_json::json;
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

async fn connect_and_migrate() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile".to_string()
    });
    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect database for backup parity test");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

fn instance_admin_actor(user_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::board_with_source(
        user_id,
        Uuid::nil(),
        ActorSource::Session,
        vec![],
        true,
    )
}

fn non_admin_board_actor(company_id: Uuid) -> AuthorizationActor {
    let user_id = Uuid::new_v4();
    AuthorizationActor::board_with_source(
        user_id,
        company_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            company_id,
            PrincipalType::User,
            user_id,
            MembershipRole::Owner,
        )],
        false,
    )
}

#[tokio::test]
async fn database_backup_health_requires_instance_admin() {
    let pool = connect_and_migrate().await;
    let state = build_app_state(pool).await.expect("build app state");
    let app = instance_settings_routes().with_state(state);
    let company_id = Uuid::new_v4();
    let non_admin = non_admin_board_actor(company_id);

    let mut request = Request::builder()
        .method("GET")
        .uri("/instance/database-backups")
        .body(Body::empty())
        .expect("build request");
    request.extensions_mut().insert(non_admin);
    let response = app.clone().oneshot(request).await.expect("dispatch");
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "non-admin must be forbidden from database backup health"
    );

    let admin = instance_admin_actor(Uuid::new_v4());
    let mut request = Request::builder()
        .method("GET")
        .uri("/instance/database-backups")
        .body(Body::empty())
        .expect("build request");
    request.extensions_mut().insert(admin);
    let response = app.oneshot(request).await.expect("dispatch");
    // Instance admin can access (backup may be not configured = 501 or healthy)
    let status = response.status();
    assert!(
        status == StatusCode::OK || status == StatusCode::NOT_IMPLEMENTED,
        "instance admin should see backup health, got {status}"
    );
}

#[tokio::test]
async fn database_backup_create_requires_instance_admin() {
    let pool = connect_and_migrate().await;
    let state = build_app_state(pool).await.expect("build app state");
    let app = instance_settings_routes().with_state(state);
    let company_id = Uuid::new_v4();
    let non_admin = non_admin_board_actor(company_id);

    let mut request = Request::builder()
        .method("POST")
        .uri("/instance/database-backups")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&json!({})).expect("serialize")))
        .expect("build request");
    request.extensions_mut().insert(non_admin);
    let response = app.clone().oneshot(request).await.expect("dispatch");
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "non-admin must be forbidden from creating backup"
    );

    let admin = instance_admin_actor(Uuid::new_v4());
    let mut request = Request::builder()
        .method("POST")
        .uri("/instance/database-backups")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&json!({})).expect("serialize")))
        .expect("build request");
    request.extensions_mut().insert(admin);
    let response = app.oneshot(request).await.expect("dispatch");
    let status = response.status();
    assert!(
        status == StatusCode::CREATED
            || status == StatusCode::NOT_IMPLEMENTED
            || status == StatusCode::CONFLICT
            || status == StatusCode::INTERNAL_SERVER_ERROR,
        "instance admin gets a reasonable response, got {status}"
    );
}

#[tokio::test]
async fn database_backup_restore_requires_instance_admin() {
    let pool = connect_and_migrate().await;
    let state = build_app_state(pool).await.expect("build app state");
    let app = instance_settings_routes().with_state(state);
    let company_id = Uuid::new_v4();
    let backup_id = Uuid::new_v4();
    let non_admin = non_admin_board_actor(company_id);

    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/instance/database-backups/{backup_id}/restore"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({"confirmation": format!("restore:{backup_id}")}))
                .expect("serialize"),
        ))
        .expect("build request");
    request.extensions_mut().insert(non_admin);
    let response = app.clone().oneshot(request).await.expect("dispatch");
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "non-admin must be forbidden from restore"
    );

    let admin = instance_admin_actor(Uuid::new_v4());
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/instance/database-backups/{backup_id}/restore"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({"confirmation": format!("restore:{backup_id}")}))
                .expect("serialize"),
        ))
        .expect("build request");
    request.extensions_mut().insert(admin);
    let response = app.oneshot(request).await.expect("dispatch");
    let status = response.status();
    assert!(
        status == StatusCode::PRECONDITION_FAILED
            || status == StatusCode::NOT_FOUND
            || status == StatusCode::INTERNAL_SERVER_ERROR,
        "instance admin gets a reasonable restore response, got {status}"
    );
}

#[tokio::test]
async fn database_backup_restore_rejects_wrong_confirmation() {
    let pool = connect_and_migrate().await;
    let state = build_app_state(pool).await.expect("build app state");
    let app = instance_settings_routes().with_state(state);
    let backup_id = Uuid::new_v4();
    let admin = instance_admin_actor(Uuid::new_v4());

    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/instance/database-backups/{backup_id}/restore"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({"confirmation": "wrong-confirmation"}))
                .expect("serialize"),
        ))
        .expect("build request");
    request.extensions_mut().insert(admin);
    let response = app.oneshot(request).await.expect("dispatch");
    assert_eq!(
        response.status(),
        StatusCode::PRECONDITION_FAILED,
        "wrong confirmation must be rejected"
    );
}

#[tokio::test]
async fn database_backup_restore_rejects_missing_confirmation() {
    let pool = connect_and_migrate().await;
    let state = build_app_state(pool).await.expect("build app state");
    let app = instance_settings_routes().with_state(state);
    let backup_id = Uuid::new_v4();
    let admin = instance_admin_actor(Uuid::new_v4());

    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/instance/database-backups/{backup_id}/restore"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({})).expect("serialize"),
        ))
        .expect("build request");
    request.extensions_mut().insert(admin);
    let response = app.oneshot(request).await.expect("dispatch");
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "missing confirmation must be bad request"
    );
}
