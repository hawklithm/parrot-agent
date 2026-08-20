use models::user_secret::UserSecret;
use repositories::user_secret_repository::{PostgresUserSecretRepository, UserSecretRepository};
use sqlx::PgPool;
use uuid::Uuid;

#[tokio::test]
async fn user_secret_update_uses_atomic_latest_version_cas() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping user secret version test: DATABASE_URL is not set");
        return;
    };
    let pool = PgPool::connect(&database_url).await.unwrap();
    let company_id = Uuid::new_v4();
    let definition_id = Uuid::new_v4();
    let secret_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("user secret CAS test company")
        .bind(format!("T{}", &company_id.simple().to_string()[..5]))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO user_secret_definitions (id, company_id, key, name, provider, managed_mode) VALUES ($1, $2, $3, $4, 'local_encrypted', 'managed')",
    )
    .bind(definition_id)
    .bind(company_id)
    .bind("cas_test")
    .bind("CAS Test")
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_secret_declarations (id, company_id, user_secret_definition_id, target_type, target_id, config_path, env_key, value_material, value_sha256) VALUES ($1, $2, $3, 'user', $4, 'env', 'CAS_TEST', to_jsonb('old'::text), 'old')",
    )
    .bind(secret_id)
    .bind(company_id)
    .bind(definition_id)
    .bind(owner_id.to_string())
    .execute(&pool)
    .await
    .unwrap();

    let repository = PostgresUserSecretRepository::new(pool.clone());
    let mut candidate: UserSecret = repository.get_secret(secret_id).await.unwrap().unwrap();
    candidate.value_material = Some("new-material".to_string());
    candidate.value_sha256 = Some("new-sha".to_string());
    let updated = repository
        .update_secret_if_version(candidate.clone(), 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.latest_version, 2);
    assert!(repository
        .update_secret_if_version(candidate, 1)
        .await
        .unwrap()
        .is_none());

    sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .unwrap();
}
