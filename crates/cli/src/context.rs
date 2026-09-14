//! Local client context profiles.
//!
//! Paperclip keeps API base/company/persona selection separate from the
//! server config.  Parrot's original CLI only had one global URL/token pair,
//! which made switching between a board profile and an agent profile
//! needlessly error-prone.  This small JSON store provides the same durable
//! profile boundary without ever persisting a bearer token in the context
//! file; tokens are resolved from the environment variable named by a profile.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, env, fs, path::PathBuf};

const CONTEXT_VERSION: u8 = 2;
const DEFAULT_PROFILE: &str = "default";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextProfile {
    pub api_base: Option<String>,
    pub company_id: Option<String>,
    pub persona: Option<String>,
    pub agent_id: Option<String>,
    pub agent_name: Option<String>,
    pub api_key_env_var_name: Option<String>,
    pub token_name: Option<String>,
    pub token_id: Option<String>,
    pub token_created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CliContext {
    pub version: u8,
    pub current_profile: String,
    pub profiles: BTreeMap<String, ContextProfile>,
}

impl Default for CliContext {
    fn default() -> Self {
        let mut profiles = BTreeMap::new();
        profiles.insert(DEFAULT_PROFILE.to_owned(), ContextProfile::default());
        Self {
            version: CONTEXT_VERSION,
            current_profile: DEFAULT_PROFILE.to_owned(),
            profiles,
        }
    }
}

pub fn resolve_context_path(explicit: Option<PathBuf>) -> Option<PathBuf> {
    explicit
        .or_else(|| env::var_os("PARROT_CONTEXT").map(PathBuf::from))
        .or_else(|| {
            crate::config::default_config_path()
                .map(|path| path.with_file_name("context.json"))
        })
}

pub fn load(explicit: Option<PathBuf>) -> Result<(PathBuf, CliContext)> {
    let path = resolve_context_path(explicit)
        .ok_or_else(|| anyhow::anyhow!("unable to determine context path"))?;
    if !path.exists() {
        return Ok((path, CliContext::default()));
    }
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to read context file {}", path.display()))?;
    let raw: serde_json::Value = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse context file {}", path.display()))?;
    Ok((path, normalize(raw)))
}

pub fn save(path: &PathBuf, context: &CliContext) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create context directory {}", parent.display()))?;
    }
    let serialized = serde_json::to_string_pretty(&normalize(
        serde_json::to_value(context).context("serialize CLI context")?,
    ))?;
    fs::write(path, format!("{serialized}\n"))
        .with_context(|| format!("failed to write context file {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn active_profile<'a>(context: &'a CliContext, requested: Option<&'a str>) -> (&'a str, &'a ContextProfile) {
    let name = requested
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&context.current_profile);
    if let Some(profile) = context.profiles.get(name) {
        return (name, profile);
    }
    let fallback = context
        .profiles
        .get(DEFAULT_PROFILE)
        .expect("normalized CLI context always contains default profile");
    (DEFAULT_PROFILE, fallback)
}

pub fn upsert_profile(
    context: &mut CliContext,
    name: &str,
    patch: ContextProfile,
    make_current: bool,
) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("context profile name must not be empty");
    }
    let existing = context.profiles.entry(name.to_owned()).or_default();
    merge_optional(&mut existing.api_base, patch.api_base);
    merge_optional(&mut existing.company_id, patch.company_id);
    merge_optional(&mut existing.persona, patch.persona);
    merge_optional(&mut existing.agent_id, patch.agent_id);
    merge_optional(&mut existing.agent_name, patch.agent_name);
    merge_optional(&mut existing.api_key_env_var_name, patch.api_key_env_var_name);
    merge_optional(&mut existing.token_name, patch.token_name);
    merge_optional(&mut existing.token_id, patch.token_id);
    merge_optional(&mut existing.token_created_at, patch.token_created_at);
    if make_current {
        context.current_profile = name.to_owned();
    }
    Ok(())
}

fn merge_optional(target: &mut Option<String>, value: Option<String>) {
    if let Some(value) = value {
        if value.trim().is_empty() {
            *target = None;
        } else {
            *target = Some(value.trim().to_owned());
        }
    }
}

fn normalize(raw: serde_json::Value) -> CliContext {
    let Some(object) = raw.as_object() else {
        return CliContext::default();
    };
    let current_profile = object
        .get("currentProfile")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(DEFAULT_PROFILE)
        .to_owned();
    let mut profiles = BTreeMap::new();
    if let Some(raw_profiles) = object.get("profiles").and_then(|value| value.as_object()) {
        for (name, raw_profile) in raw_profiles {
            if name.trim().is_empty() {
                continue;
            }
            profiles.insert(name.clone(), normalize_profile(raw_profile));
        }
    }
    if profiles.is_empty() {
        profiles.insert(DEFAULT_PROFILE.to_owned(), ContextProfile::default());
    }
    profiles
        .entry(current_profile.clone())
        .or_insert_with(ContextProfile::default);
    CliContext {
        version: CONTEXT_VERSION,
        current_profile,
        profiles,
    }
}

fn normalize_profile(raw: &serde_json::Value) -> ContextProfile {
    let Some(object) = raw.as_object() else {
        return ContextProfile::default();
    };
    let string = |key: &str| {
        object
            .get(key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let persona = string("persona").filter(|value| matches!(value.as_str(), "board" | "agent"));
    ContextProfile {
        api_base: string("apiBase"),
        company_id: string("companyId"),
        persona,
        agent_id: string("agentId"),
        agent_name: string("agentName"),
        api_key_env_var_name: string("apiKeyEnvVarName"),
        token_name: string("tokenName"),
        token_id: string("tokenId"),
        token_created_at: string("tokenCreatedAt"),
    }
}

pub fn profile_token(profile: &ContextProfile) -> Option<String> {
    let key_name = profile.api_key_env_var_name.as_deref()?;
    env::var(key_name).ok().filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_invalid_context_to_a_default_profile() {
        let context = normalize(serde_json::json!({"profiles": {"bad": {"persona": "other"}}}));
        assert_eq!(context.current_profile, "default");
        assert!(context.profiles.contains_key("default"));
        assert_eq!(context.profiles["bad"].persona, None);
    }

    #[test]
    fn merges_profile_values_and_switches_current_profile() {
        let mut context = CliContext::default();
        upsert_profile(
            &mut context,
            "agent",
            ContextProfile {
                api_base: Some("http://localhost:3100".to_owned()),
                persona: Some("agent".to_owned()),
                ..Default::default()
            },
            true,
        )
        .unwrap();
        assert_eq!(context.current_profile, "agent");
        assert_eq!(context.profiles["agent"].persona.as_deref(), Some("agent"));
    }
}
