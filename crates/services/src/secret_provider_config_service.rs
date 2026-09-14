//! Secret provider configuration service.
//!
//! Mirrors Paperclip's `server/src/services/secrets.ts` provider-vault section:
//! one row per stored config, a partial-unique default per `(company_id, provider)`,
//! hard delete returning the removed row, and deployment-level provider health.
//!
//! Error split, which is observable over HTTP and therefore part of the
//! contract rather than an implementation detail:
//!   * `invalid_request` → `400`. Paperclip rejects these in a route-level
//!     `validate(schema)` middleware, i.e. before any service call.
//!   * `unprocessable` → `422`. Paperclip raises these from inside the service.
//!
//! The create and update schemas are NOT the same, which is why the same
//! condition can be 400 on POST and 422 on PATCH:
//!   * `createSecretProviderConfigSchema.superRefine` re-parses the provider
//!     payload, so an unknown config key is 400 on create.
//!   * `updateSecretProviderConfigSchema.superRefine` only pre-checks the
//!     sensitive-key pattern and the vault address, leaving the rest to the
//!     service — where the identical unknown key surfaces as 422.

use async_trait::async_trait;
use models::{
    CompanySecretProviderConfig, CreateSecretProviderConfigRequest, SecretProvider,
    SecretProviderConfigDiscoveryPreviewRequest, SecretProviderConfigDiscoveryPreviewResult,
    SecretProviderConfigHealthDetails, SecretProviderConfigHealthResponse,
    SecretProviderConfigHealthStatus, SecretProviderConfigStatus, SecretProviderDescriptor,
    SecretProviderHealthCheck, SecretProviderHealthResponse, SecretProviderHealthStatus,
    UpdateSecretProviderConfigRequest,
};
use regex::Regex;
use repositories::{RepositoryError, SecretProviderConfigRepository};
use serde_json::{Map, Value as JsonValue};
use std::sync::{Arc, OnceLock};
use uuid::Uuid;

use crate::errors::{ServiceError, ServiceResult};

/// Providers whose runtime is locked: they accept draft metadata only, and any
/// stored config is forced into `coming_soon` unless explicitly `disabled`.
/// Paperclip `COMING_SOON_SECRET_PROVIDERS` (`secrets.ts:78-81`).
const COMING_SOON_PROVIDERS: [SecretProvider; 2] =
    [SecretProvider::GcpSecretManager, SecretProvider::Vault];

/// Short free-text config values (Paperclip `safeShortText`, max 160 chars).
const SAFE_SHORT_TEXT_MAX: usize = 160;

/// Config keys that must never be persisted: they would durably store
/// credentials in a readable column.
/// Paperclip `deniedProviderConfigKeyPattern` (`validators/secret.ts:227-229`).
fn denied_provider_config_key_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"(?i)^(access[-_]?key([-_]?id)?|secret[-_]?access[-_]?key|secret[-_]?key|token|password|passwd|credential|credentials|private[-_]?key|pem|jwt|session[-_]?token|service[-_]?account([-_]?json)?|client[-_]?secret|secret[-_]?id|unseal[-_]?key|recovery[-_]?key|key[-_]?file([-_]?path)?|token[-_]?file([-_]?path)?)$",
        )
        .expect("denied provider config key pattern must compile")
    })
}

/// Service for managing secret provider configurations.
#[async_trait]
pub trait SecretProviderConfigService: Send + Sync {
    /// All provider configurations for a company, newest first.
    async fn list_configs(
        &self,
        company_id: Uuid,
    ) -> ServiceResult<Vec<CompanySecretProviderConfig>>;

    /// Preview secret discovery from an external provider.
    async fn discovery_preview(
        &self,
        company_id: Uuid,
        request: SecretProviderConfigDiscoveryPreviewRequest,
    ) -> ServiceResult<SecretProviderConfigDiscoveryPreviewResult>;
    /// Static capability descriptors for every provider module, in registry
    /// order. Not company-scoped: the company path param only gates access.
    fn list_providers(&self) -> Vec<SecretProviderDescriptor>;
    async fn create_config(
        &self,
        company_id: Uuid,
        request: CreateSecretProviderConfigRequest,
        actor_user_id: Option<String>,
    ) -> ServiceResult<CompanySecretProviderConfig>;

    /// Fetch a single provider configuration.
    async fn get_config(&self, config_id: Uuid) -> ServiceResult<CompanySecretProviderConfig>;

    /// Apply a partial update to a provider configuration.
    async fn update_config(
        &self,
        config_id: Uuid,
        request: UpdateSecretProviderConfigRequest,
    ) -> ServiceResult<CompanySecretProviderConfig>;

    /// Hard-delete a provider configuration, returning the removed row.
    async fn delete_config(&self, config_id: Uuid) -> ServiceResult<CompanySecretProviderConfig>;

    /// Make a configuration the default for its `(company, provider)`.
    async fn set_default(&self, config_id: Uuid) -> ServiceResult<CompanySecretProviderConfig>;

    /// Health-check one stored configuration.
    async fn health_check(
        &self,
        config_id: Uuid,
    ) -> ServiceResult<SecretProviderConfigHealthResponse>;

    /// Deployment-level health of every provider module, in registry order.
    ///
    /// The payload is not company-scoped; the route only uses the company to
    /// enforce access control (Paperclip's `checkSecretProviders`).
    async fn company_health(&self) -> ServiceResult<SecretProviderHealthResponse>;
}

/// Default implementation backed by [`SecretProviderConfigRepository`].
pub struct DefaultSecretProviderConfigServiceImpl {
    repo: Arc<dyn SecretProviderConfigRepository>,
}

impl DefaultSecretProviderConfigServiceImpl {
    pub fn new(repo: Arc<dyn SecretProviderConfigRepository>) -> Self {
        Self { repo }
    }

    /// Map a persisted row onto the API model.
    ///
    /// `provider` and `status` are stored as text; an unrecognized value means
    /// the row was written outside this service, so it surfaces as an internal
    /// error rather than being coerced into a valid-looking default.
    fn to_api_model(db: models::SecretProviderConfig) -> ServiceResult<CompanySecretProviderConfig> {
        let provider = parse_provider(&db.provider)
            .ok_or_else(|| ServiceError::Internal(format!("unknown provider {:?}", db.provider)))?;
        let status = parse_status(&db.status)
            .ok_or_else(|| ServiceError::Internal(format!("unknown status {:?}", db.status)))?;
        let health_status = db.health_status.as_deref().and_then(parse_health_status);
        let health_details: Option<SecretProviderConfigHealthDetails> = db
            .health_details
            .clone()
            .and_then(|value| serde_json::from_value(value).ok());

        Ok(CompanySecretProviderConfig {
            id: db.id,
            company_id: db.company_id,
            provider,
            display_name: db.display_name,
            status,
            is_default: db.is_default,
            config: db.config,
            health_status,
            health_checked_at: db.health_checked_at,
            health_message: db.health_message,
            health_details,
            disabled_at: db.disabled_at,
            created_by_agent_id: db.created_by_agent_id,
            created_by_user_id: db.created_by_user_id,
            created_at: db.created_at,
            updated_at: db.updated_at,
        })
    }

    fn map_err(e: RepositoryError) -> ServiceError {
        match e {
            RepositoryError::NotFound(id) => {
                ServiceError::NotFound(format!("Config {id} not found"))
            }
            RepositoryError::DatabaseError(e) => ServiceError::Internal(e.to_string()),
            RepositoryError::InvalidData(msg) => ServiceError::Unprocessable(msg),
        }
    }

    /// Health that can be answered without probing the provider module.
    ///
    /// Returns `None` for states that permit a real probe, letting the caller
    /// fall through to the provider module's own health check.
    fn static_config_health(
        config: &models::SecretProviderConfig,
    ) -> Option<SecretProviderConfigHealthResponse> {
        let provider = parse_provider(&config.provider)?;
        let checked_at = chrono::Utc::now();

        if config.status == SecretProviderConfigStatus::Disabled.as_str() {
            let message = "Provider vault is disabled.";
            return Some(SecretProviderConfigHealthResponse {
                config_id: config.id,
                provider,
                status: SecretProviderConfigHealthStatus::Disabled,
                message: message.to_string(),
                details: health_details("disabled", message, Vec::new()),
                checked_at,
            });
        }

        if config.status == SecretProviderConfigStatus::ComingSoon.as_str()
            || is_coming_soon(provider)
        {
            let message = "Provider vault runtime is locked while coming soon.";
            return Some(SecretProviderConfigHealthResponse {
                config_id: config.id,
                provider,
                status: SecretProviderConfigHealthStatus::ComingSoon,
                message: message.to_string(),
                details: health_details(
                    "runtime_locked",
                    message,
                    vec![
                        "Draft metadata may be saved, but create, rotate, and resolve stay unavailable."
                            .to_string(),
                    ],
                ),
                checked_at,
            });
        }

        None
    }
}

fn is_coming_soon(provider: SecretProvider) -> bool {
    COMING_SOON_PROVIDERS.contains(&provider)
}

/// Build a config health details block. Empty vectors are skipped on
/// serialization, so this matches Paperclip's optional-field shape.
fn health_details(
    code: &str,
    message: &str,
    guidance: Vec<String>,
) -> SecretProviderConfigHealthDetails {
    SecretProviderConfigHealthDetails {
        code: code.to_string(),
        message: message.to_string(),
        missing_fields: Vec::new(),
        guidance,
    }
}

fn default_status_for(provider: SecretProvider) -> SecretProviderConfigStatus {
    if is_coming_soon(provider) {
        SecretProviderConfigStatus::ComingSoon
    } else {
        SecretProviderConfigStatus::Ready
    }
}

fn parse_provider(raw: &str) -> Option<SecretProvider> {
    SecretProvider::ALL.into_iter().find(|p| p.as_str() == raw)
}

fn parse_status(raw: &str) -> Option<SecretProviderConfigStatus> {
    match raw {
        "ready" => Some(SecretProviderConfigStatus::Ready),
        "warning" => Some(SecretProviderConfigStatus::Warning),
        "coming_soon" => Some(SecretProviderConfigStatus::ComingSoon),
        "disabled" => Some(SecretProviderConfigStatus::Disabled),
        _ => None,
    }
}

fn parse_health_status(raw: &str) -> Option<SecretProviderConfigHealthStatus> {
    match raw {
        "ready" => Some(SecretProviderConfigHealthStatus::Ready),
        "warning" => Some(SecretProviderConfigHealthStatus::Warning),
        "error" => Some(SecretProviderConfigHealthStatus::Error),
        "coming_soon" => Some(SecretProviderConfigHealthStatus::ComingSoon),
        "disabled" => Some(SecretProviderConfigHealthStatus::Disabled),
        _ => None,
    }
}

/// Semantic refusal decided by the service (Paperclip `unprocessable`) → 422.
fn unprocessable(message: impl Into<String>) -> ServiceError {
    ServiceError::Unprocessable(message.into())
}

/// Request-shape refusal (Paperclip's `validate(...)` middleware) → 400.
fn invalid_request(message: impl Into<String>) -> ServiceError {
    ServiceError::InvalidInput(message.into())
}

/// Move a create-time config failure into the 400 vocabulary.
fn as_request_error(error: ServiceError) -> ServiceError {
    match error {
        ServiceError::Unprocessable(message) => invalid_request(message),
        other => other,
    }
}

/// Provider-specific config validation.
///
/// Every Paperclip provider schema is `.strict()`, so an unknown key is a
/// rejection, not a passthrough.
fn validate_provider_config(provider: SecretProvider, config: &JsonValue) -> ServiceResult<JsonValue> {
    reject_sensitive_config_keys(config)?;

    let empty = Map::new();
    let object = config.as_object().unwrap_or(&empty);

    match provider {
        SecretProvider::LocalEncrypted => {
            let mut out = Map::new();
            for (key, value) in object {
                match key.as_str() {
                    "backupReminderAcknowledged" if value.is_boolean() => {
                        out.insert(key.clone(), value.clone());
                    }
                    _ => return Err(unknown_config_field(key)),
                }
            }
            Ok(JsonValue::Object(out))
        }
        SecretProvider::AwsSecretsManager => {
            let mut out = Map::new();
            for (key, value) in object {
                match key.as_str() {
                    "region" => {
                        let trimmed = value
                            .as_str()
                            .map(str::trim)
                            .filter(|region| is_valid_aws_region(region))
                            .ok_or_else(|| invalid_config_field(key))?;
                        out.insert(key.clone(), JsonValue::String(trimmed.to_string()));
                    }
                    "namespace" | "secretNamePrefix" | "ownerTag" | "environmentTag" => {
                        out.insert(key.clone(), short_text(key, value)?);
                    }
                    "kmsKeyId" => {
                        out.insert(key.clone(), long_text(key, value, 512)?);
                    }
                    _ => return Err(unknown_config_field(key)),
                }
            }
            // `region` is the one required key in Paperclip's AWS schema
            // (`region: z.string().trim().regex(...)`, with no `.optional()`),
            // so its absence is a rejection, not a default.
            if !out.contains_key("region") {
                return Err(invalid_config_field("region"));
            }
            Ok(JsonValue::Object(out))
        }
        SecretProvider::GcpSecretManager => {
            let mut out = Map::new();
            for (key, value) in object {
                match key.as_str() {
                    "projectId" => {
                        if value.is_null() {
                            out.insert(key.clone(), JsonValue::Null);
                        } else {
                            let trimmed = value
                                .as_str()
                                .map(str::trim)
                                .filter(|id| is_valid_gcp_project_id(id))
                                .ok_or_else(|| invalid_config_field(key))?;
                            out.insert(key.clone(), JsonValue::String(trimmed.to_string()));
                        }
                    }
                    "location" | "namespace" | "secretNamePrefix" => {
                        out.insert(key.clone(), short_text(key, value)?);
                    }
                    _ => return Err(unknown_config_field(key)),
                }
            }
            Ok(JsonValue::Object(out))
        }
        SecretProvider::Vault => {
            let mut out = Map::new();
            for (key, value) in object {
                match key.as_str() {
                    "address" => {
                        if value.is_null() {
                            out.insert(key.clone(), JsonValue::Null);
                        } else {
                            let text = value.as_str().ok_or_else(|| invalid_config_field(key))?;
                            out.insert(key.clone(), JsonValue::String(normalize_vault_address(text)?));
                        }
                    }
                    "namespace" | "mountPath" | "secretPathPrefix" => {
                        out.insert(key.clone(), short_text(key, value)?);
                    }
                    _ => return Err(unknown_config_field(key)),
                }
            }
            Ok(JsonValue::Object(out))
        }
    }
}

/// Reject any config key that looks like a credential.
fn reject_sensitive_config_keys(config: &JsonValue) -> ServiceResult<()> {
    let Some(object) = config.as_object() else {
        return Err(unprocessable("Invalid provider vault config"));
    };
    for key in object.keys() {
        if denied_provider_config_key_pattern().is_match(key) {
            return Err(unprocessable(format!(
                "Provider vault config cannot persist sensitive field: {key}"
            )));
        }
    }
    Ok(())
}

/// `createSecretProviderConfigSchema`: rejects sensitive keys, then re-parses the
/// provider payload. Every failure is 400.
fn validate_provider_config_for_create(
    provider: SecretProvider,
    config: &JsonValue,
) -> ServiceResult<JsonValue> {
    validate_provider_config(provider, config).map_err(as_request_error)
}

/// `updateSecretProviderConfigSchema`: rejects sensitive keys and an unsafe vault
/// address, nothing else. Unknown keys and bad values are left to the service,
/// so they surface as 422.
fn validate_provider_config_for_update(
    provider: SecretProvider,
    config: &JsonValue,
) -> ServiceResult<JsonValue> {
    let Some(object) = config.as_object() else {
        return Err(invalid_request("Invalid provider vault config"));
    };
    for key in object.keys() {
        if denied_provider_config_key_pattern().is_match(key) {
            return Err(invalid_request(format!(
                "Provider vault config cannot persist sensitive field: {key}"
            )));
        }
    }
    if let Some(address) = object.get("address").filter(|value| !value.is_null()) {
        normalize_vault_address(address.as_str().unwrap_or_default()).map_err(as_request_error)?;
    }
    validate_provider_config(provider, config)
}

fn invalid_config_field(key: &str) -> ServiceError {
    unprocessable(format!("Invalid provider vault config: config.{key}"))
}

fn unknown_config_field(key: &str) -> ServiceError {
    unprocessable(format!(
        "Invalid provider vault config: unrecognized key `config.{key}`"
    ))
}

/// `optionalSafeShortText`: trimmed 1..=160 chars, or null.
fn short_text(key: &str, value: &JsonValue) -> ServiceResult<JsonValue> {
    long_text(key, value, SAFE_SHORT_TEXT_MAX)
}

/// Trimmed 1..=`max` chars, or null for an explicit `null`.
fn long_text(key: &str, value: &JsonValue, max: usize) -> ServiceResult<JsonValue> {
    if value.is_null() {
        return Ok(JsonValue::Null);
    }
    let text = value
        .as_str()
        .ok_or_else(|| invalid_config_field(key))?
        .trim();
    if text.is_empty() || text.chars().count() > max {
        return Err(invalid_config_field(key));
    }
    Ok(JsonValue::String(text.to_string()))
}

/// `/^[a-z]{2}(?:-gov)?-[a-z]+-\d+$/`
fn is_valid_aws_region(region: &str) -> bool {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN
        .get_or_init(|| Regex::new(r"^[a-z]{2}(?:-gov)?-[a-z]+-\d+$").expect("aws region pattern"))
        .is_match(region)
}

/// `/^[a-z][a-z0-9-]{4,127}$/`
fn is_valid_gcp_project_id(project_id: &str) -> bool {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN
        .get_or_init(|| Regex::new(r"^[a-z][a-z0-9-]{4,127}$").expect("gcp project pattern"))
        .is_match(project_id)
}

/// Vault address must be an origin-only HTTP(S) URL, and is normalized to its
/// origin (Paperclip `vaultAddressSchema`).
fn normalize_vault_address(address: &str) -> ServiceResult<String> {
    const REJECTED: &str = "Vault address must be an origin-only HTTP(S) URL without \
                            credentials, path, query, or fragment";

    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r"(?i)^(https?)://([^/?#]+)([/?#].*)?$").expect("vault url pattern")
    });

    let trimmed = address.trim();
    let Some(caps) = pattern.captures(trimmed) else {
        return Err(unprocessable(REJECTED));
    };
    let scheme = caps.get(1).expect("group 1 always participates").as_str();
    let authority = caps.get(2).expect("group 2 always participates").as_str();

    // A trailing bare `/` is the origin; anything else is a path/query/fragment.
    if authority.contains('@') {
        return Err(unprocessable(REJECTED));
    }
    match caps.get(3).map(|m| m.as_str()) {
        None | Some("/") => {}
        Some(_) => return Err(unprocessable(REJECTED)),
    }

    Ok(format!("{}://{}", scheme.to_ascii_lowercase(), authority))
}

#[async_trait]
impl SecretProviderConfigService for DefaultSecretProviderConfigServiceImpl {
    fn list_providers(&self) -> Vec<SecretProviderDescriptor> {
        secret_provider_descriptors()
    }

    async fn list_configs(
        &self,
        company_id: Uuid,
    ) -> ServiceResult<Vec<CompanySecretProviderConfig>> {
        let configs = self
            .repo
            .list_by_company(company_id)
            .await
            .map_err(Self::map_err)?;
        configs.into_iter().map(Self::to_api_model).collect()
    }

    async fn discovery_preview(
        &self,
        company_id: Uuid,
        request: SecretProviderConfigDiscoveryPreviewRequest,
    ) -> ServiceResult<SecretProviderConfigDiscoveryPreviewResult> {
        // The discovery route re-parses the payload against the create schema,
        // so shape violations are 400 before the provider is even consulted.
        validate_provider_config_for_create(request.provider, &request.config)
            .map_err(as_request_error)?;

        // `discoverProviderConfigs` is optional on a provider module and only
        // the AWS module implements it. Everyone else is a semantic refusal.
        let SecretProvider::AwsSecretsManager = request.provider else {
            return Err(unprocessable(format!(
                "{} provider does not support provider vault discovery",
                request.provider
            )));
        };

        // Parrot's AWS client speaks the JSON 1.1 API but has no ListSecrets
        // call, so discovery is an honest gap rather than an empty preview.
        let _ = (company_id, request.query, request.next_token, request.page_size);
        Err(ServiceError::NotImplemented(
            "aws_secrets_manager provider vault discovery (ListSecrets)".to_string(),
        ))
    }

    async fn create_config(
        &self,
        company_id: Uuid,
        request: CreateSecretProviderConfigRequest,
        actor_user_id: Option<String>,
    ) -> ServiceResult<CompanySecretProviderConfig> {
        let provider = request.provider;
        let status = request.status.unwrap_or_else(|| default_status_for(provider));

        // Mirrors `createSecretProviderConfigSchema.superRefine`, which runs
        // before any row is touched: these are 400, not 422.
        if is_coming_soon(provider) && !status.blocks_default() {
            return Err(invalid_request(format!(
                "{provider} provider vaults are locked while coming soon"
            )));
        }
        let is_default = request.is_default.unwrap_or(false);
        if status.blocks_default() && is_default {
            return Err(invalid_request(
                "Only ready or warning provider vaults can be default",
            ));
        }

        let display_name = request.display_name.trim();
        if display_name.is_empty() || display_name.chars().count() > 120 {
            return Err(invalid_request("Invalid provider vault config: displayName"));
        }
        let config = validate_provider_config_for_create(provider, &request.config)?;

        let created = self
            .repo
            .create(models::CreateSecretProviderConfigInput {
                company_id,
                provider: provider.as_str().to_string(),
                display_name: display_name.to_string(),
                status: status.as_str().to_string(),
                is_default,
                config,
                created_by_agent_id: None,
                created_by_user_id: actor_user_id,
            })
            .await
            .map_err(Self::map_err)?;

        // Persisting a new default must clear the previous default for this
        // `(company_id, provider)`, which `set_default` does transactionally.
        let created = if is_default {
            self.repo
                .set_default(created.id)
                .await
                .map_err(Self::map_err)?
                .ok_or_else(|| {
                    ServiceError::Internal("created default provider config vanished".to_string())
                })?
        } else {
            created
        };

        Self::to_api_model(created)
    }

    async fn get_config(&self, config_id: Uuid) -> ServiceResult<CompanySecretProviderConfig> {
        self.repo
            .get_by_id(config_id)
            .await
            .map_err(Self::map_err)?
            .ok_or_else(provider_vault_not_found)
            .and_then(Self::to_api_model)
    }

    async fn update_config(
        &self,
        config_id: Uuid,
        request: UpdateSecretProviderConfigRequest,
    ) -> ServiceResult<CompanySecretProviderConfig> {
        let existing = self
            .repo
            .get_by_id(config_id)
            .await
            .map_err(Self::map_err)?
            .ok_or_else(provider_vault_not_found)?;
        let provider = parse_provider(&existing.provider).ok_or_else(|| {
            ServiceError::Internal(format!("unknown provider {:?}", existing.provider))
        })?;

        let status = match request.status {
            Some(status) => status,
            None => parse_status(&existing.status).ok_or_else(|| {
                ServiceError::Internal(format!("unknown status {:?}", existing.status))
            })?,
        };

        // Service-side checks (422).
        if is_coming_soon(provider) && !status.blocks_default() {
            return Err(unprocessable(format!(
                "{provider} provider vaults are locked while coming soon"
            )));
        }
        // Schema-side check (400), matching the PATCH superRefine.
        if status.blocks_default() && request.is_default == Some(true) {
            return Err(invalid_request(
                "Only ready or warning provider vaults can be default",
            ));
        }

        let display_name = match request.display_name.as_deref().map(str::trim) {
            Some(name) if !name.is_empty() && name.chars().count() <= 120 => {
                Some(name.to_string())
            }
            Some(_) => return Err(invalid_request("Invalid provider vault config: displayName")),
            None => None,
        };

        let config = match &request.config {
            Some(raw) => Some(validate_provider_config_for_update(provider, raw)?),
            None => None,
        };

        // A status that forbids being default always drops the flag; otherwise an
        // explicit value wins over the stored one.
        let is_default = if status.blocks_default() {
            Some(false)
        } else {
            request.is_default
        };

        let updated = self
            .repo
            .update(
                config_id,
                models::UpdateSecretProviderConfigInput {
                    display_name,
                    status: Some(status.as_str().to_string()),
                    is_default,
                    config,
                },
            )
            .await
            .map_err(Self::map_err)?
            .ok_or_else(provider_vault_not_found)?;

        // Promoting to default clears peers of the same provider.
        let updated = if is_default == Some(true) {
            self.repo
                .set_default(updated.id)
                .await
                .map_err(Self::map_err)?
                .ok_or_else(provider_not_defaultable)?
        } else {
            updated
        };

        Self::to_api_model(updated)
    }

    async fn delete_config(&self, config_id: Uuid) -> ServiceResult<CompanySecretProviderConfig> {
        self.repo
            .delete(config_id)
            .await
            .map_err(Self::map_err)?
            .ok_or_else(provider_vault_not_found)
            .and_then(Self::to_api_model)
    }

    async fn set_default(&self, config_id: Uuid) -> ServiceResult<CompanySecretProviderConfig> {
        let existing = self
            .repo
            .get_by_id(config_id)
            .await
            .map_err(Self::map_err)?
            .ok_or_else(provider_vault_not_found)?;

        let provider = parse_provider(&existing.provider).ok_or_else(|| {
            ServiceError::Internal(format!("unknown provider {:?}", existing.provider))
        })?;
        let status = parse_status(&existing.status).ok_or_else(|| {
            ServiceError::Internal(format!("unknown status {:?}", existing.status))
        })?;
        if is_coming_soon(provider) || status.blocks_default() {
            return Err(provider_not_defaultable());
        }

        self.repo
            .set_default(config_id)
            .await
            .map_err(Self::map_err)?
            .ok_or_else(provider_not_defaultable)
            .and_then(Self::to_api_model)
    }

    async fn health_check(
        &self,
        config_id: Uuid,
    ) -> ServiceResult<SecretProviderConfigHealthResponse> {
        let db_config = self
            .repo
            .get_by_id(config_id)
            .await
            .map_err(Self::map_err)?
            .ok_or_else(provider_vault_not_found)?;

        if let Some(static_health) = Self::static_config_health(&db_config) {
            return Ok(static_health);
        }

        let provider = parse_provider(&db_config.provider).ok_or_else(|| {
            ServiceError::Internal(format!("unknown provider {:?}", db_config.provider))
        })?;
        let status = parse_status(&db_config.status).ok_or_else(|| {
            ServiceError::Internal(format!("unknown status {:?}", db_config.status))
        })?;

        // Probe the deployment-level provider module, then persist the outcome.
        let checked_at = chrono::Utc::now();
        let response = map_module_health(
            config_id,
            provider,
            status,
            &provider_module_health(provider),
            checked_at,
        );

        let details = serde_json::to_value(&response.details).unwrap_or(JsonValue::Null);
        self.repo
            .record_health(
                config_id,
                health_status_str(response.status),
                &response.message,
                &details,
                checked_at,
            )
            .await
            .map_err(Self::map_err)?;

        Ok(response)
    }

    async fn company_health(&self) -> ServiceResult<SecretProviderHealthResponse> {
        Ok(SecretProviderHealthResponse {
            providers: SecretProvider::ALL
                .into_iter()
                .map(provider_module_health)
                .collect(),
        })
    }
}

/// Paperclip `notFound("Provider vault not found")`.
fn provider_vault_not_found() -> ServiceError {
    ServiceError::NotFound("Provider vault not found".to_string())
}

/// Paperclip `unprocessable("Only ready or warning provider vaults can be default")`.
fn provider_not_defaultable() -> ServiceError {
    unprocessable("Only ready or warning provider vaults can be default")
}

fn health_status_str(status: SecretProviderConfigHealthStatus) -> &'static str {
    match status {
        SecretProviderConfigHealthStatus::Ready => "ready",
        SecretProviderConfigHealthStatus::Warning => "warning",
        SecretProviderConfigHealthStatus::Error => "error",
        SecretProviderConfigHealthStatus::ComingSoon => "coming_soon",
        SecretProviderConfigHealthStatus::Disabled => "disabled",
    }
}

/// Fold a provider module health check into a per-config health response.
fn map_module_health(
    config_id: Uuid,
    provider: SecretProvider,
    provider_status: SecretProviderConfigStatus,
    health: &SecretProviderHealthCheck,
    checked_at: chrono::DateTime<chrono::Utc>,
) -> SecretProviderConfigHealthResponse {
    let status = match health.status {
        // A config already flagged `warning` stays a warning even when the
        // deployment probe succeeds: the operator has not cleared it yet.
        SecretProviderHealthStatus::Ok => match provider_status {
            SecretProviderConfigStatus::Warning => SecretProviderConfigHealthStatus::Warning,
            _ => SecretProviderConfigHealthStatus::Ready,
        },
        SecretProviderHealthStatus::Warn => SecretProviderConfigHealthStatus::Warning,
        SecretProviderHealthStatus::Error => SecretProviderConfigHealthStatus::Error,
    };

    // Provider guidance is flat in Paperclip's details block, so the module's
    // warnings and backup advice merge into one list.
    let mut guidance: Vec<String> = health.warnings.clone();
    guidance.extend(health.backup_guidance.iter().cloned());

    SecretProviderConfigHealthResponse {
        config_id,
        provider,
        status,
        message: health.message.clone(),
        details: health_details(
            match status {
                SecretProviderConfigHealthStatus::Ready => "provider_ready",
                _ => "provider_needs_attention",
            },
            &health.message,
            guidance,
        ),
        checked_at,
    }
}

/// Deployment-level health of one provider module.
fn provider_module_health(provider: SecretProvider) -> SecretProviderHealthCheck {
    match provider {
        SecretProvider::LocalEncrypted => local_encrypted_health(),
        SecretProvider::AwsSecretsManager => aws_secrets_manager_health(),
        SecretProvider::GcpSecretManager | SecretProvider::Vault => {
            coming_soon_module_health(provider)
        }
    }
}

/// Paperclip `inspectLocalEncryptedHealth` (`secrets/local-encrypted-provider.ts:109`).
fn local_encrypted_health() -> SecretProviderHealthCheck {
    const ENV_KEY: &str = "PARROT_SECRET_ENCRYPTION_KEY";

    fn backup_guidance() -> Vec<String> {
        vec![
            "Back up the key file together with database backups.".to_string(),
            "The database alone cannot restore local encrypted secret values.".to_string(),
        ]
    }

    let env_value = std::env::var(ENV_KEY)
        .ok()
        .filter(|value| !value.trim().is_empty());
    if let Some(raw) = env_value {
        // The live encryptor accepts 64 hex chars; anything else makes it fall
        // back to a zero key, which is exactly the condition worth surfacing.
        if raw.len() != 64 || !raw.chars().all(|c| c.is_ascii_hexdigit()) {
            return SecretProviderHealthCheck {
                provider: SecretProvider::LocalEncrypted,
                status: SecretProviderHealthStatus::Error,
                message: format!("{ENV_KEY} is invalid; expected 64 hex characters (32 bytes)"),
                warnings: Vec::new(),
                backup_guidance: Vec::new(),
                details: Some(serde_json::json!({ "keySource": "env" })),
            };
        }
        return SecretProviderHealthCheck {
            provider: SecretProvider::LocalEncrypted,
            status: SecretProviderHealthStatus::Ok,
            message: format!("Local encrypted provider is using {ENV_KEY}"),
            warnings: Vec::new(),
            backup_guidance: backup_guidance(),
            details: Some(serde_json::json!({ "keySource": "env" })),
        };
    }

    let key_path = crate::config::SecretsConfig::default()
        .master_key_path
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| "./data/master.key".to_string());
    let details = || serde_json::json!({ "keySource": "file", "keyFilePath": key_path });

    let metadata = match std::fs::metadata(&key_path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return SecretProviderHealthCheck {
                provider: SecretProvider::LocalEncrypted,
                status: SecretProviderHealthStatus::Warn,
                message: format!("Secrets key file does not exist yet: {key_path}"),
                warnings: vec![
                    "The first managed secret write will create this key file with 0600 permissions."
                        .to_string(),
                ],
                backup_guidance: backup_guidance(),
                details: Some(details()),
            };
        }
        Err(err) => {
            return SecretProviderHealthCheck {
                provider: SecretProvider::LocalEncrypted,
                status: SecretProviderHealthStatus::Error,
                message: format!("Could not read secrets key file: {err}"),
                warnings: Vec::new(),
                backup_guidance: Vec::new(),
                details: Some(details()),
            };
        }
    };

    // Group/other permission bits on the key file are a real exposure.
    let warnings = match key_file_mode(&metadata) {
        Some(mode) if mode & 0o077 != 0 => vec![format!(
            "Secrets key file permissions are {mode:o}; run chmod 600 {key_path}"
        )],
        _ => Vec::new(),
    };

    SecretProviderHealthCheck {
        provider: SecretProvider::LocalEncrypted,
        status: if warnings.is_empty() {
            SecretProviderHealthStatus::Ok
        } else {
            SecretProviderHealthStatus::Warn
        },
        message: format!("Local encrypted provider configured with key file {key_path}"),
        warnings,
        backup_guidance: backup_guidance(),
        details: Some(details()),
    }
}

#[cfg(unix)]
fn key_file_mode(metadata: &std::fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(metadata.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn key_file_mode(_metadata: &std::fs::Metadata) -> Option<u32> {
    None
}

/// Env vars the AWS runtime reads, in Paperclip's precedence order.
fn aws_region_from_env() -> Option<String> {
    ["PARROT_AWS_REGION", "AWS_REGION", "AWS_DEFAULT_REGION"]
        .into_iter()
        .find_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

/// Credential sources the AWS SDK default chain would find, described the way
/// Paperclip's `describeDetectedAwsCredentialSources` does.
fn detected_aws_credential_sources() -> Vec<String> {
    fn non_empty(name: &str) -> bool {
        std::env::var(name).is_ok_and(|value| !value.trim().is_empty())
    }

    let mut sources = Vec::new();
    if non_empty("AWS_PROFILE") {
        sources.push("AWS_PROFILE/shared config".to_string());
    }
    if non_empty("PARROT_AWS_ACCESS_KEY_ID") && non_empty("PARROT_AWS_SECRET_ACCESS_KEY") {
        sources.push("Parrot environment credentials".to_string());
    }
    if non_empty("AWS_ACCESS_KEY_ID") && non_empty("AWS_SECRET_ACCESS_KEY") {
        sources.push(
            "temporary AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY environment credentials".to_string(),
        );
    }
    if non_empty("AWS_WEB_IDENTITY_TOKEN_FILE") && non_empty("AWS_ROLE_ARN") {
        sources.push("AWS web identity token".to_string());
    }
    if non_empty("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI")
        || non_empty("AWS_CONTAINER_CREDENTIALS_FULL_URI")
    {
        sources.push("AWS container credentials endpoint".to_string());
    }
    if non_empty("AWS_SHARED_CREDENTIALS_FILE") || non_empty("AWS_CONFIG_FILE") {
        sources.push("custom AWS shared credentials/config file".to_string());
    }
    sources
}

/// Non-secret deployment configuration the AWS provider needs before it can
/// serve managed values (Paperclip `getAwsConfigReadiness.missingConfig`).
fn aws_missing_config() -> Vec<String> {
    let mut missing = Vec::new();
    if aws_region_from_env().is_none() {
        missing.push("PARROT_AWS_REGION or AWS_REGION/AWS_DEFAULT_REGION".to_string());
    }
    if std::env::var("PARROT_SECRETS_AWS_DEPLOYMENT_ID").is_err() {
        missing.push("PARROT_SECRETS_AWS_DEPLOYMENT_ID".to_string());
    }
    if std::env::var("PARROT_SECRETS_AWS_KMS_KEY_ID").is_err() {
        missing.push("PARROT_SECRETS_AWS_KMS_KEY_ID".to_string());
    }
    missing
}

/// Paperclip `aws-secrets-manager-provider.ts:1032` `healthCheck()`.
///
/// The module reports `warn` (never `error`) whenever deployment config is
/// incomplete, with the missing keys enumerated in `details.missingConfig`.
fn aws_secrets_manager_health() -> SecretProviderHealthCheck {
    fn non_empty(name: &str) -> bool {
        std::env::var(name).is_ok_and(|value| !value.trim().is_empty())
    }

    const CREDENTIAL_CUSTODY_WARNING: &str = "AWS credentials stay outside Parrot: managed \
        secret values are stored in AWS Secrets Manager and referenced by id.";

    let detected = detected_aws_credential_sources();
    let backup_guidance = vec![
        "Back up Parrot metadata separately from AWS-managed secrets.".to_string(),
        "Restoring access requires the Parrot database plus the same AWS secret namespace and KMS permissions."
            .to_string(),
    ];

    let missing = aws_missing_config();
    if !missing.is_empty() {
        return SecretProviderHealthCheck {
            provider: SecretProvider::AwsSecretsManager,
            status: SecretProviderHealthStatus::Warn,
            message: format!(
                "AWS Secrets Manager provider is not ready: missing {}.",
                missing.join(", ")
            ),
            warnings: vec![
                format!(
                    "Missing required non-secret AWS provider config: {}.",
                    missing.join(", ")
                ),
                CREDENTIAL_CUSTODY_WARNING.to_string(),
                "Managed secret create/rotate/resolve calls will fail until AWS provider \
                 configuration is complete."
                    .to_string(),
            ],
            backup_guidance,
            details: Some(serde_json::json!({
                "missingConfig": missing,
                "requiredProviderConfig": [],
                "optionalProviderConfig": [
                    "PARROT_SECRETS_AWS_PREFIX",
                    "PARROT_SECRETS_AWS_ENVIRONMENT",
                    "PARROT_SECRETS_AWS_PROVIDER_OWNER",
                    "PARROT_SECRETS_AWS_ENDPOINT",
                ],
                "credentialSource": "AWS SDK default credential provider chain",
                "detectedCredentialSources": detected,
            })),
        };
    }

    let region = aws_region_from_env().unwrap_or_default();
    let deployment_id = std::env::var("PARROT_SECRETS_AWS_DEPLOYMENT_ID").unwrap_or_default();
    let prefix = std::env::var("PARROT_SECRETS_AWS_PREFIX").unwrap_or_default();
    let kms_key_id = std::env::var("PARROT_SECRETS_AWS_KMS_KEY_ID").unwrap_or_default();

    let mut warnings = Vec::new();
    if prefix.trim().is_empty() {
        warnings.push(
            "PARROT_SECRETS_AWS_PREFIX should be set to a deployment-scoped prefix".to_string(),
        );
    }
    if non_empty("AWS_ACCESS_KEY_ID") && non_empty("AWS_SECRET_ACCESS_KEY") {
        warnings.push(
            "AWS static environment credentials are visible to this process; use only short-lived \
             shell credentials locally and prefer IAM role/workload identity for hosted \
             deployments."
                .to_string(),
        );
    }

    SecretProviderHealthCheck {
        provider: SecretProvider::AwsSecretsManager,
        status: if warnings.is_empty() {
            SecretProviderHealthStatus::Ok
        } else {
            SecretProviderHealthStatus::Warn
        },
        message: "AWS Secrets Manager provider config is present; AWS credentials are resolved by \
                  the server runtime through the AWS SDK default credential provider chain."
            .to_string(),
        warnings,
        backup_guidance,
        details: Some(serde_json::json!({
            "region": region,
            "prefix": prefix,
            "deploymentId": deployment_id,
            "kmsKeyConfigured": !kms_key_id.trim().is_empty(),
            "credentialSource": "AWS SDK default credential provider chain",
            "detectedCredentialSources": detected,
        })),
    }
}

/// `GET /companies/:companyId/secret-providers` body: one static descriptor per
/// registry entry, in registry order (Paperclip `listSecretProviders`).
pub fn secret_provider_descriptors() -> Vec<SecretProviderDescriptor> {
    SecretProvider::ALL
        .into_iter()
        .map(|provider| {
            // Only AWS derives `configured` from deployment state; the others
            // are fixed by their module definitions.
            let (label, requires_external_ref, supports_managed_values, configured) = match provider {
                SecretProvider::LocalEncrypted => ("Local encrypted (default)", false, true, true),
                SecretProvider::AwsSecretsManager => (
                    "AWS Secrets Manager",
                    false,
                    true,
                    aws_missing_config().is_empty(),
                ),
                SecretProvider::GcpSecretManager => ("GCP Secret Manager", true, false, false),
                SecretProvider::Vault => ("HashiCorp Vault", true, false, false),
            };
            SecretProviderDescriptor {
                id: provider,
                label: label.to_string(),
                requires_external_ref,
                supports_managed_values,
                // `local_encrypted` cannot link external references; the other
                // three can.
                supports_external_references: provider != SecretProvider::LocalEncrypted,
                // Only AWS can write values through to the upstream provider.
                supports_external_value_writes: provider == SecretProvider::AwsSecretsManager,
                configured,
            }
        })
        .collect()
}

/// `gcp_secret_manager` and `vault` ship as external-reference-only modules.
/// Paperclip `external-stub-providers.ts:66-87`.
fn coming_soon_module_health(provider: SecretProvider) -> SecretProviderHealthCheck {
    SecretProviderHealthCheck {
        provider,
        status: SecretProviderHealthStatus::Warn,
        message: format!(
            "{provider} provider is available for external references but not configured for \
             runtime resolution"
        ),
        warnings: vec![
            "Linked external references can be stored as metadata, but runtime resolution will fail \
             until this provider is configured."
                .to_string(),
        ],
        backup_guidance: Vec::new(),
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_status_is_coming_soon_only_for_locked_providers() {
        assert_eq!(
            default_status_for(SecretProvider::LocalEncrypted),
            SecretProviderConfigStatus::Ready
        );
        assert_eq!(
            default_status_for(SecretProvider::AwsSecretsManager),
            SecretProviderConfigStatus::Ready
        );
        assert_eq!(
            default_status_for(SecretProvider::GcpSecretManager),
            SecretProviderConfigStatus::ComingSoon
        );
        assert_eq!(
            default_status_for(SecretProvider::Vault),
            SecretProviderConfigStatus::ComingSoon
        );
    }

    #[test]
    fn sensitive_config_keys_are_rejected_for_every_provider() {
        for provider in SecretProvider::ALL {
            for key in [
                "keyPath",
                "accessKeyId",
                "secret_access_key",
                "password",
                "token",
                "privateKey",
            ] {
                let config = serde_json::json!({ key: "value" });
                let err = validate_provider_config(provider, &config)
                    .expect_err("sensitive key must be rejected");
                assert!(
                    matches!(err, ServiceError::Unprocessable(_)),
                    "{provider} / {key} produced {err:?}"
                );
            }
        }
    }

    #[test]
    fn unknown_config_keys_are_rejected() {
        // Paperclip's provider schemas are `.strict()`.
        let err = validate_provider_config(
            SecretProvider::AwsSecretsManager,
            &serde_json::json!({ "region": "us-east-1", "secretPrefix": "parrot/" }),
        )
        .expect_err("unknown key must be rejected");
        assert!(matches!(err, ServiceError::Unprocessable(_)), "{err:?}");
    }

    /// The create/update split is the whole reason `as_request_error` exists:
    /// the identical payload must be 400 on POST and 422 on PATCH.
    #[test]
    fn create_reports_config_failures_as_400_and_update_as_422() {
        let unknown_key = serde_json::json!({ "nope": 1 });
        assert!(matches!(
            validate_provider_config_for_create(SecretProvider::LocalEncrypted, &unknown_key),
            Err(ServiceError::InvalidInput(_))
        ));
        assert!(matches!(
            validate_provider_config_for_update(SecretProvider::LocalEncrypted, &unknown_key),
            Err(ServiceError::Unprocessable(_))
        ));

        // A sensitive key and an unsafe vault address are schema-level on both
        // verbs, so both are 400.
        let sensitive = serde_json::json!({ "token": "x" });
        assert!(matches!(
            validate_provider_config_for_create(SecretProvider::LocalEncrypted, &sensitive),
            Err(ServiceError::InvalidInput(_))
        ));
        assert!(matches!(
            validate_provider_config_for_update(SecretProvider::LocalEncrypted, &sensitive),
            Err(ServiceError::InvalidInput(_))
        ));

        let bad_address = serde_json::json!({ "address": "https://vault.example.com/path" });
        assert!(matches!(
            validate_provider_config_for_update(SecretProvider::Vault, &bad_address),
            Err(ServiceError::InvalidInput(_))
        ));
    }

    #[test]
    fn aws_region_must_match_paperclip_pattern() {
        assert!(validate_provider_config(
            SecretProvider::AwsSecretsManager,
            &serde_json::json!({ "region": "us-east-1" })
        )
        .is_ok());
        for bad in ["us_gov_east", "USA-EAST-1", "us-east"] {
            assert!(
                validate_provider_config(
                    SecretProvider::AwsSecretsManager,
                    &serde_json::json!({ "region": bad })
                )
                .is_err(),
                "{bad} must be rejected"
            );
        }
    }

    #[test]
    fn vault_address_is_normalized_to_origin() {
        let config = validate_provider_config(
            SecretProvider::Vault,
            &serde_json::json!({ "address": "https://vault.example.com:8200/" }),
        )
        .expect("origin-only address is valid");
        assert_eq!(config["address"], "https://vault.example.com:8200");

        for bad in [
            "https://user:pass@vault.example.com:8200",
            "https://vault.example.com:8200/secret",
            "https://vault.example.com:8200?x=1",
            "ftp://vault.example.com:8200",
            "vault.example.com:8200",
        ] {
            assert!(
                validate_provider_config(SecretProvider::Vault, &serde_json::json!({ "address": bad }))
                    .is_err(),
                "{bad} must be rejected"
            );
        }
    }

    #[test]
    fn local_encrypted_accepts_only_the_backup_acknowledgement_flag() {
        assert!(validate_provider_config(
            SecretProvider::LocalEncrypted,
            &serde_json::json!({ "backupReminderAcknowledged": true })
        )
        .is_ok());
        assert!(validate_provider_config(
            SecretProvider::LocalEncrypted,
            &serde_json::json!({ "backupReminderAcknowledged": "yes" })
        )
        .is_err());
    }

    #[test]
    fn coming_soon_module_health_is_external_reference_only() {
        let health = coming_soon_module_health(SecretProvider::Vault);
        assert_eq!(health.provider, SecretProvider::Vault);
        assert_eq!(health.status, SecretProviderHealthStatus::Warn);
        assert!(!health.warnings.is_empty());
    }

    #[test]
    fn module_health_maps_onto_config_health_statuses() {
        let checked_at = chrono::Utc::now();
        let health = SecretProviderHealthCheck {
            provider: SecretProvider::LocalEncrypted,
            status: SecretProviderHealthStatus::Ok,
            message: "ok".to_string(),
            warnings: Vec::new(),
            backup_guidance: Vec::new(),
            details: None,
        };
        let mapped = map_module_health(
            Uuid::nil(),
            SecretProvider::LocalEncrypted,
            SecretProviderConfigStatus::Ready,
            &health,
            checked_at,
        );
        assert_eq!(mapped.status, SecretProviderConfigHealthStatus::Ready);

        // An operator-acknowledged warning must survive a successful probe.
        let mapped = map_module_health(
            Uuid::nil(),
            SecretProvider::LocalEncrypted,
            SecretProviderConfigStatus::Warning,
            &health,
            checked_at,
        );
        assert_eq!(mapped.status, SecretProviderConfigHealthStatus::Warning);
    }
}
