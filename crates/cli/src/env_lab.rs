//! Deterministic local environment fixture used by CLI smoke tests.
//!
//! Parrot does not ship Paperclip's SSH adapter-utils package.  We still keep
//! the env-lab contract useful by providing a local, inspectable fixture with
//! explicit state and lifecycle commands.  Adapter-specific integration tests
//! can point at the emitted workspace path without depending on global state.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvLabState {
    pub fixture_dir: String,
    pub workspace_dir: String,
    pub created_at: String,
}

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str).unwrap_or("status") {
        "up" | "start" => up(args),
        "down" | "stop" => down(args),
        "status" => status(args),
        "doctor" => doctor(args),
        _ => {
            println!("Usage: parrot env-lab up | status | doctor | down [--json]");
            Ok(())
        }
    }
}

fn up(args: &[String]) -> Result<()> {
    let root = root(args)?;
    let workspace = root.join("workspace");
    fs::create_dir_all(&workspace)
        .with_context(|| format!("failed to create env-lab workspace {}", workspace.display()))?;
    let state = EnvLabState {
        fixture_dir: root.display().to_string(),
        workspace_dir: workspace.display().to_string(),
        created_at: Utc::now().to_rfc3339(),
    };
    write_state(&root, &state)?;
    print_state(&state, args.iter().any(|arg| arg == "--json"));
    Ok(())
}

fn status(args: &[String]) -> Result<()> {
    let root = root(args)?;
    let state = read_state(&root)?;
    if args.iter().any(|arg| arg == "--json") {
        println!("{}", serde_json::json!({"running": state.is_some(), "state": state}));
    } else if let Some(state) = state {
        println!("env-lab: running");
        println!("workspace: {}", state.workspace_dir);
    } else {
        println!("env-lab: stopped");
    }
    Ok(())
}

fn doctor(args: &[String]) -> Result<()> {
    let root = root(args)?;
    let state = read_state(&root)?;
    let workspace_exists = state
        .as_ref()
        .map(|value| PathBuf::from(&value.workspace_dir).is_dir())
        .unwrap_or(false);
    let payload = serde_json::json!({
        "fixtureRoot": root,
        "statePresent": state.is_some(),
        "workspacePresent": workspace_exists,
        "status": if workspace_exists { "pass" } else { "warn" },
    });
    if args.iter().any(|arg| arg == "--json") {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("env-lab: {}", payload["status"].as_str().unwrap_or("unknown"));
        println!("fixture root: {}", root.display());
        if !workspace_exists {
            println!("hint: run `parrot env-lab up`");
        }
    }
    Ok(())
}

fn down(args: &[String]) -> Result<()> {
    let root = root(args)?;
    let existed = root.exists();
    if existed {
        fs::remove_dir_all(&root)
            .with_context(|| format!("failed to remove env-lab fixture {}", root.display()))?;
    }
    if args.iter().any(|arg| arg == "--json") {
        println!("{}", serde_json::json!({"stopped": existed, "fixtureRoot": root}));
    } else {
        println!("{}", if existed { "env-lab: stopped" } else { "env-lab: already stopped" });
    }
    Ok(())
}

fn root(args: &[String]) -> Result<PathBuf> {
    if let Some(path) = args.windows(2).find(|window| window[0] == "--dir") {
        return Ok(PathBuf::from(&path[1]));
    }
    if let Some(path) = env::var_os("PARROT_ENV_LAB_DIR") {
        return Ok(PathBuf::from(path));
    }
    Ok(env::temp_dir().join("parrot-env-lab"))
}

fn state_path(root: &PathBuf) -> PathBuf {
    root.join("state.json")
}

fn read_state(root: &PathBuf) -> Result<Option<EnvLabState>> {
    let path = state_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    Ok(Some(serde_json::from_str(&raw).context("parse env-lab state")?))
}

fn write_state(root: &PathBuf, state: &EnvLabState) -> Result<()> {
    fs::write(state_path(root), format!("{}\n", serde_json::to_string_pretty(state)?))?;
    Ok(())
}

fn print_state(state: &EnvLabState, json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(state).unwrap_or_default());
    } else {
        println!("env-lab: running");
        println!("workspace: {}", state.workspace_dir);
        println!("fixture: {}", state.fixture_dir);
    }
}
