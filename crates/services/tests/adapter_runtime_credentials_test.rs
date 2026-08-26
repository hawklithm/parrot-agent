//! Integration coverage for runtime adapter credential provisioning.

use serde_json::json;
use services::secret_provider::{encrypt_secret_material, LocalEncryptedProvider};
use services::{AdapterRuntimeSecretResolver, DatabaseAdapterRuntimeSecretResolver};
use sqlx::PgPool;
use uuid::Uuid;

async fn connect() -> Option<PgPool> {
    let database_url = std::env::var("DATABASE_URL").ok()?;
    match PgPool::connect(&database_url).await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!("Skipping adapter runtime credential test: {error}");
            None
        }
    }
}

#[tokio::test]
async fn resolves_company_and_responsible_user_credentials_before_launch() {
    let Some(pool) = connect().await else {
        return;
    };
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    std::env::set_var("PARROT_SECRET_ENCRYPTION_KEY", "0".repeat(64));

    let company_id = Uuid::new_v4();
    let owner_user_id = Uuid::new_v4();
    let company_prefix = format!("AR{}", &company_id.simple().to_string()[..8]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Adapter Runtime Credentials Co")
        .bind(&company_prefix)
        .execute(&pool)
        .await
        .expect("insert company");
    sqlx::query("INSERT INTO auth_users (id, email, name) VALUES ($1, $2, 'Credential Owner')")
        .bind(owner_user_id)
        .bind(format!("{}@example.test", owner_user_id))
        .execute(&pool)
        .await
        .expect("insert owner");

    let company_secret_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO company_secrets
         (id, company_id, scope, key, name, provider, status, managed_mode, latest_version)
         VALUES ($1, $2, 'company', 'RUNTIME_API_KEY', 'Runtime API Key',
                 'local_encrypted', 'active', 'paperclip_managed', 1)",
    )
    .bind(company_secret_id)
    .bind(company_id)
    .execute(&pool)
    .await
    .expect("insert company secret");
    let (company_material, company_sha) = encrypt_secret_material("company-token")
        .expect("encrypt company secret");
    sqlx::query(
        "INSERT INTO company_secret_versions
         (secret_id, version, material, value_sha256, fingerprint_sha256, status)
         VALUES ($1, 1, $2, $3, $3, 'current')",
    )
    .bind(company_secret_id)
    .bind(company_material)
    .bind(company_sha)
    .execute(&pool)
    .await
    .expect("insert company secret version");

    let definition_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_secret_definitions (id, company_id, key, name, required)
         VALUES ($1, $2, 'PERSONAL_TOKEN', 'Personal Token', true)",
    )
    .bind(definition_id)
    .bind(company_id)
    .execute(&pool)
    .await
    .expect("insert user secret definition");
    let user_provider = LocalEncryptedProvider::new(vec![0; 32]).expect("create provider");
    let user_ciphertext = user_provider
        .encrypt("user-token")
        .expect("encrypt user secret");
    sqlx::query(
        "INSERT INTO user_secret_declarations
         (id, company_id, user_secret_definition_id, target_type, target_id,
          config_path, env_key, value_material, value_sha256, latest_version)
         VALUES ($1, $2, $3, 'user', $4, 'env', 'PERSONAL_TOKEN', $5, $6, 1)",
    )
    .bind(Uuid::new_v4())
    .bind(company_id)
    .bind(definition_id)
    .bind(owner_user_id.to_string())
    .bind(json!(user_ciphertext))
    .bind("user-token-sha")
    .execute(&pool)
    .await
    .expect("insert user secret declaration");

    let config = json!({
        "apiKey": {
            "type": "secret_ref",
            "secretId": company_secret_id,
            "version": "latest"
        },
        "env": {
            "PERSONAL_TOKEN": {
                "type": "user_secret_ref",
                "key": "PERSONAL_TOKEN",
                "version": "latest"
            }
        }
    });
    let resolver = DatabaseAdapterRuntimeSecretResolver::new(pool.clone());
    let resolved = resolver
        .resolve_adapter_config(company_id, Some(owner_user_id), config.clone())
        .await
        .expect("resolve runtime credentials");

    assert_eq!(resolved.config["apiKey"], "company-token");
    assert_eq!(resolved.config["env"]["PERSONAL_TOKEN"], "user-token");
    assert_eq!(config["apiKey"]["secretId"], company_secret_id.to_string());
    assert!(resolved.secret_keys.iter().any(|key| key == "apiKey"));
    assert!(resolved
        .secret_keys
        .iter()
        .any(|key| key == "env.PERSONAL_TOKEN"));
    assert!(resolved.manifest.iter().any(|entry| {
        entry.secret_id == Some(company_secret_id)
            && entry.outcome == services::SecretResolutionOutcome::Success
    }));
    assert!(resolved.manifest.iter().any(|entry| {
        entry.user_secret_definition_id == Some(definition_id)
            && entry.outcome == services::SecretResolutionOutcome::Success
    }));

    let missing_owner_error = resolver
        .resolve_adapter_config(
            company_id,
            None,
            json!({
                "env": {
                    "PERSONAL_TOKEN": {
                        "type": "user_secret_ref",
                        "key": "PERSONAL_TOKEN"
                    }
                }
            }),
        )
        .await
        .expect_err("required user secret without an owner must fail closed");
    assert!(missing_owner_error
        .to_string()
        .contains("responsible user is required"));

    sqlx::query("DELETE FROM user_secret_declarations WHERE company_id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM user_secret_definitions WHERE company_id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM company_secrets WHERE company_id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM auth_users WHERE id = $1")
        .bind(owner_user_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .ok();
}
