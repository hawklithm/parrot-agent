use async_trait::async_trait;
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::secret_provider::decrypt_secret_material;
use crate::secret_service::{
    EnvBinding, ResolvedAdapterConfig, RuntimeSecretManifestEntry, SecretResolutionOutcome,
    SecretServiceError,
};

fn decrypt_runtime_secret_material(material: &Value) -> Result<String, SecretServiceError> {
    // UserSecretService stores the ciphertext as a JSON string, while company
    // secrets store the same ciphertext inside a material envelope.
    let envelope = match material {
        Value::String(raw) => {
            let ciphertext = raw
                .strip_prefix("local:")
                .and_then(|value| value.split_once(':').map(|(_, ciphertext)| ciphertext))
                .unwrap_or(raw);
            serde_json::json!({ "ciphertext": ciphertext })
        }
        _ => material.clone(),
    };
    decrypt_secret_material(&envelope)
        .map_err(|error| SecretServiceError::ResolutionFailed(error.to_string()))
}

/// Resolves persisted adapter bindings immediately before an adapter starts.
/// The resolved config is intentionally short-lived and is never written back
/// to the agent row.
#[async_trait]
pub trait AdapterRuntimeSecretResolver: Send + Sync {
    async fn resolve_adapter_config(
        &self,
        company_id: Uuid,
        responsible_user_id: Option<Uuid>,
        adapter_config: Value,
    ) -> Result<ResolvedAdapterConfig, SecretServiceError>;
}

pub struct DatabaseAdapterRuntimeSecretResolver {
    pool: PgPool,
}

impl DatabaseAdapterRuntimeSecretResolver {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn resolve_company_secret(
        &self,
        company_id: Uuid,
        secret_id: Uuid,
        requested_version: &str,
    ) -> Result<(String, String, i32), SecretServiceError> {
        let requested_version = if requested_version.trim().is_empty() {
            "latest"
        } else {
            requested_version
        };

        let row = if requested_version.eq_ignore_ascii_case("latest") {
            sqlx::query(
                "SELECT v.material, v.version, s.key, s.provider
                 FROM company_secret_versions v
                 JOIN company_secrets s ON s.id = v.secret_id
                 WHERE s.id = $1
                   AND s.company_id = $2
                   AND s.status = 'active'
                   AND s.deleted_at IS NULL
                   AND v.revoked_at IS NULL
                 ORDER BY v.version DESC
                 LIMIT 1",
            )
            .bind(secret_id)
            .bind(company_id)
            .fetch_optional(&self.pool)
            .await?
        } else {
            let version = requested_version.parse::<i32>().map_err(|_| {
                SecretServiceError::ResolutionFailed(format!(
                    "invalid secret version '{requested_version}'"
                ))
            })?;
            if version < 1 {
                return Err(SecretServiceError::ResolutionFailed(
                    "secret version must be positive".to_string(),
                ));
            }
            sqlx::query(
                "SELECT v.material, v.version, s.key, s.provider
                 FROM company_secret_versions v
                 JOIN company_secrets s ON s.id = v.secret_id
                 WHERE s.id = $1
                   AND s.company_id = $2
                   AND s.status = 'active'
                   AND s.deleted_at IS NULL
                   AND v.version = $3
                   AND v.revoked_at IS NULL
                 LIMIT 1",
            )
            .bind(secret_id)
            .bind(company_id)
            .bind(version)
            .fetch_optional(&self.pool)
            .await?
        };

        let row = row.ok_or_else(|| SecretServiceError::SecretNotFound(secret_id.to_string()))?;
        let provider: String = row.try_get("provider")?;
        if provider != "local_encrypted" && provider != "local" {
            return Err(SecretServiceError::ResolutionFailed(format!(
                "secret provider '{provider}' is not available to the local adapter runtime"
            )));
        }
        let material: Value = row.try_get("material")?;
        let value = decrypt_runtime_secret_material(&material)?;
        let key: String = row.try_get("key")?;
        let version: i32 = row.try_get("version")?;
        Ok((value, key, version))
    }

    async fn resolve_user_secret(
        &self,
        company_id: Uuid,
        responsible_user_id: Option<Uuid>,
        user_key: &str,
        requested_version: &str,
        required: bool,
        allow_missing_override: bool,
    ) -> Result<Option<(String, Uuid, String, i32)>, SecretServiceError> {
        let Some(responsible_user_id) = responsible_user_id else {
            if required && !allow_missing_override {
                return Err(SecretServiceError::ResolutionFailed(
                    "responsible user is required for user secret resolution".to_string(),
                ));
            }
            return Ok(None);
        };

        let row = sqlx::query(
            "SELECT d.id AS definition_id,
                    d.key,
                    d.required AS definition_required,
                    declaration.value_material,
                    declaration.latest_version,
                    declaration.required AS declaration_required,
                    declaration.allow_missing_override AS declaration_allow_missing
             FROM user_secret_definitions d
             LEFT JOIN user_secret_declarations declaration
               ON declaration.user_secret_definition_id = d.id
              AND declaration.company_id = d.company_id
              AND declaration.target_type = 'user'
              AND declaration.target_id = $3
             WHERE d.company_id = $1
               AND d.key = $2
               AND d.status = 'active'
               AND d.deleted_at IS NULL
             LIMIT 1",
        )
        .bind(company_id)
        .bind(user_key)
        .bind(responsible_user_id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            if required && !allow_missing_override {
                return Err(SecretServiceError::ResolutionFailed(format!(
                    "required user secret '{user_key}' is not configured"
                )));
            }
            return Ok(None);
        };

        let definition_id: Uuid = row.try_get("definition_id")?;
        let definition_required: bool = row.try_get("definition_required")?;
        let declaration_required: Option<bool> = row.try_get("declaration_required")?;
        let declaration_allow_missing: Option<bool> = row.try_get("declaration_allow_missing")?;
        let effective_required = (required
            || definition_required
            || declaration_required.unwrap_or(false))
            && !(allow_missing_override || declaration_allow_missing.unwrap_or(false));
        let material: Option<Value> = row.try_get("value_material")?;
        let Some(material) = material else {
            if effective_required {
                return Err(SecretServiceError::ResolutionFailed(format!(
                    "required user secret '{user_key}' is not configured"
                )));
            }
            return Ok(None);
        };

        let latest_version: i32 = row
            .try_get::<Option<i32>, _>("latest_version")?
            .unwrap_or(1);
        let requested_version = if requested_version.trim().is_empty() {
            "latest"
        } else {
            requested_version
        };
        if !requested_version.eq_ignore_ascii_case("latest") {
            let version = requested_version.parse::<i32>().map_err(|_| {
                SecretServiceError::ResolutionFailed(format!(
                    "invalid user secret version '{requested_version}'"
                ))
            })?;
            if version != latest_version {
                return Err(SecretServiceError::ResolutionFailed(format!(
                    "user secret '{user_key}' version {version} is not available"
                )));
            }
        }

        let value = decrypt_runtime_secret_material(&material)?;
        if value.is_empty() {
            if effective_required {
                return Err(SecretServiceError::ResolutionFailed(format!(
                    "required user secret '{user_key}' is empty"
                )));
            }
            return Ok(None);
        }

        let key: String = row.try_get("key")?;
        Ok(Some((value, definition_id, key, latest_version)))
    }

    async fn resolve_binding(
        &self,
        company_id: Uuid,
        responsible_user_id: Option<Uuid>,
        config_path: &str,
        env_key: Option<&str>,
        binding: EnvBinding,
    ) -> Result<(Option<String>, Option<RuntimeSecretManifestEntry>), SecretServiceError> {
        match binding.canonicalize() {
            EnvBinding::Plain { value } => Ok((Some(value), None)),
            EnvBinding::SecretRef { secret_id, version } => {
                let (value, secret_key, resolved_version) = self
                    .resolve_company_secret(company_id, secret_id, &version)
                    .await?;
                Ok((
                    Some(value),
                    Some(RuntimeSecretManifestEntry {
                        config_path: config_path.to_string(),
                        env_key: env_key.map(str::to_owned),
                        secret_id: Some(secret_id),
                        user_secret_definition_id: None,
                        secret_key,
                        version: resolved_version.to_string(),
                        outcome: SecretResolutionOutcome::Success,
                        error_code: None,
                    }),
                ))
            }
            EnvBinding::UserSecretRef {
                key,
                version,
                required,
                allow_missing_override,
            } => {
                let resolved = self
                    .resolve_user_secret(
                        company_id,
                        responsible_user_id,
                        &key,
                        &version,
                        required,
                        allow_missing_override,
                    )
                    .await?;
                let Some((value, definition_id, secret_key, resolved_version)) = resolved else {
                    return Ok((
                        None,
                        Some(RuntimeSecretManifestEntry {
                            config_path: config_path.to_string(),
                            env_key: env_key.map(str::to_owned),
                            secret_id: None,
                            user_secret_definition_id: None,
                            secret_key: key,
                            version,
                            outcome: SecretResolutionOutcome::Failure,
                            error_code: Some("USER_SECRET_MISSING".to_string()),
                        }),
                    ));
                };
                Ok((
                    Some(value),
                    Some(RuntimeSecretManifestEntry {
                        config_path: config_path.to_string(),
                        env_key: env_key.map(str::to_owned),
                        secret_id: None,
                        user_secret_definition_id: Some(definition_id),
                        secret_key,
                        version: resolved_version.to_string(),
                        outcome: SecretResolutionOutcome::Success,
                        error_code: None,
                    }),
                ))
            }
        }
    }
}

#[async_trait]
impl AdapterRuntimeSecretResolver for DatabaseAdapterRuntimeSecretResolver {
    async fn resolve_adapter_config(
        &self,
        company_id: Uuid,
        responsible_user_id: Option<Uuid>,
        adapter_config: Value,
    ) -> Result<ResolvedAdapterConfig, SecretServiceError> {
        let config_obj = adapter_config.as_object().ok_or_else(|| {
            SecretServiceError::InvalidBinding("adapter_config must be an object".to_string())
        })?;
        let mut resolved = config_obj.clone();
        let mut secret_keys = Vec::new();
        let mut manifest = Vec::new();

        if let Some(env_value) = config_obj.get("env") {
            let env_obj = env_value.as_object().ok_or_else(|| {
                SecretServiceError::InvalidBinding("env must be an object".to_string())
            })?;
            let mut resolved_env = serde_json::Map::new();
            for (key, value) in env_obj {
                let binding = EnvBinding::from_value(value)?;
                let path = format!("env.{key}");
                let (resolved_value, entry) = self
                    .resolve_binding(
                        company_id,
                        responsible_user_id,
                        &path,
                        Some(key),
                        binding,
                    )
                    .await?;
                if let Some(value) = resolved_value {
                    resolved_env.insert(key.clone(), Value::String(value));
                }
                if let Some(entry) = entry {
                    if entry.outcome == SecretResolutionOutcome::Success {
                        secret_keys.push(path);
                    }
                    manifest.push(entry);
                }
            }
            resolved.insert("env".to_string(), Value::Object(resolved_env));
        }

        for (key, value) in config_obj {
            if key == "env" {
                continue;
            }
            let Some(object) = value.as_object() else {
                continue;
            };
            if object.get("type").is_none() {
                continue;
            }
            let binding = EnvBinding::from_value(value)?;
            let (resolved_value, entry) = self
                .resolve_binding(company_id, responsible_user_id, key, None, binding)
                .await?;
            match resolved_value {
                Some(value) => {
                    resolved.insert(key.clone(), Value::String(value));
                }
                None => {
                    resolved.remove(key);
                }
            }
            if let Some(entry) = entry {
                if entry.outcome == SecretResolutionOutcome::Success {
                    secret_keys.push(key.clone());
                }
                manifest.push(entry);
            }
        }

        Ok(ResolvedAdapterConfig {
            config: Value::Object(resolved),
            secret_keys,
            manifest,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::EnvBinding;
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn accepts_frontend_camel_case_secret_reference() {
        let secret_id = Uuid::new_v4();
        let binding = EnvBinding::from_value(&json!({
            "type": "secret_ref",
            "secretId": secret_id,
            "version": "latest"
        }))
        .expect("camel-case secret reference should deserialize");
        assert!(matches!(binding, EnvBinding::SecretRef { secret_id: id, .. } if id == secret_id));
    }
}
