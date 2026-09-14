use anyhow::{bail, Context, Result};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
    thread,
    time::Duration,
};

use crate::{
    backup, checks, client::ApiClient, config::resolve_config_path, env_lab, update_notice,
    worktree,
};

pub fn run(args: impl IntoIterator<Item = String>) -> Result<()> {
    let raw: Vec<String> = args.into_iter().collect();
    let (command, rest) = match raw.first() {
        None => return print_help(),
        Some(c) => (c.as_str(), &raw[1..]),
    };
    match command {
        "help" | "--help" | "-h" => print_help(),
        "version" | "--version" | "-V" => get_version(),
        "doctor" => cmd_doctor(rest),
        "env" => cmd_env(rest),
        "channels" => cmd_channels(rest),
        "configure" => cmd_configure(rest),
        "connect" => cmd_connect(rest),
        "config" => cmd_config(rest),
        "fix" => cmd_fix(rest),
        "db-backup" => backup::run(rest),
        "auth" => cmd_auth(rest),
        "context" => cmd_context(rest),
        "token" => cmd_token(rest),
        "prompt" | "agent-prompt" => cmd_prompt(rest),
        "company" => cmd_company(rest),
        "agent" => cmd_agent(rest),
        "issue" => cmd_issue(rest),
        "goal" => cmd_goal(rest),
        "project" => cmd_project(rest),
        "secret" => cmd_secret(rest),
        "routine" => cmd_routine(rest),
        "activity" => cmd_activity(rest),
        "approval" => cmd_approval(rest),
        "pipeline" => cmd_pipeline(rest),
        "skill" => cmd_skill(rest),
        "team" => cmd_team(rest),
        "plugin" => cmd_plugin(rest),
        "dashboard" => cmd_dashboard(rest),
        "cost" => cmd_cost(rest),
        "feedback" => cmd_feedback(rest),
        "access" => cmd_access(rest),
        "workspace" => cmd_workspace(rest),
        "run" => cmd_run(rest),
        "channel" => cmd_channel(rest),
        "heartbeat" => cmd_heartbeat(rest),
        "worktree" => worktree::run(rest),
        "env-lab" => env_lab::run(rest),
        "allowed-hostname" => cmd_allowed_hostname(rest),
        "zip" => cmd_zip(rest),
        "service" => cmd_service(rest),
        "install" => cmd_install(rest),
        "uninstall" => cmd_uninstall(rest),
        "update" => cmd_update(rest),
        "onboard" => cmd_onboard(rest),
        _ => bail!("unknown command '{command}'. Run 'parrot help' for usage."),
    }
}

fn get_version() -> Result<()> {
    println!("parrot {}", env!("CARGO_PKG_VERSION"));
    // Check for updates if enabled
    let _ = update_notice::print_update_notice();
    Ok(())
}

fn print_help() -> Result<()> {
    println!("parrot {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Usage: parrot <command> [options]");
    println!();
    println!("Configuration:");
    println!("  env         Show deployment environment resolution");
    println!("  channels    Show release channels");
    println!("  configure   --server-url URL [--api-token TOKEN]");
    println!("  connect     --server-url URL [--api-token TOKEN] [--profile NAME]");
    println!("  doctor      [--json]");
    println!();
    println!("Data commands:");
    println!("  auth        get-session | bootstrap-ceo");
    println!("  context     show | list | use <profile> | set [options]");
    println!("  token       agent|board create | list | revoke");
    println!("  prompt      <text...> [--agent-id ID] [--company-id ID]");
    println!("  company     list | get <id> | create | delete <id> | export <id> | import");
    println!("  agent       list <companyId> | get <companyId> <agentId>");
    println!("  issue       list <companyId> | get <companyId> <issueId>");
    println!("  goal        list <companyId>");
    println!("  project     list <companyId>");
    println!("  secret      list <companyId>");
    println!("  routine     list <companyId>");
    println!("  activity    get <companyId>");
    println!(
        "  approval    list <companyId> | get <id> | approve <id> | reject <id> | resubmit <id>"
    );
    println!("  pipeline    list <companyId> | get <id>");
    println!("  skill       list | get <name>");
    println!("  team        catalog");
    println!("  plugin      list | create <name> --output <dir> | install | enable | disable");
    println!("  dashboard   get <companyId>");
    println!("  cost        summary <companyId>");
    println!("  feedback    get <traceId>");
    println!("  access      org-chart <companyId>");
    println!("  workspace   list <companyId> | get <id>");
    println!("  run         list <issueId> | get <id>");
    println!("  channel     list <companyId>");
    println!("  heartbeat   run --agent-id ID [--source SOURCE] [--trigger TRIGGER]");
    println!("  worktree    list | status | create | remove | env");
    println!("  env-lab     up | status | doctor | down");
    println!("  allowed-hostname add <host> | list");
    println!("  zip         <companyId> [--output FILE]  (export bundle)");
    println!();
    println!("Server management:");
    println!("  service     status | start | stop | restart | log [LINES]");
    println!("  install     [--dir PATH] [--install-service] [--service-dir PATH]");
    println!("  update      [--version VERSION]");
    println!("  uninstall   Remove installed parrot");
    println!("  onboard     Interactive setup wizard");
    println!("  fix         Diagnose and repair issues");
    println!();
    println!("Maintenance:");
    println!("  db-backup   [--dir PATH] [--retention-days N]");
    println!("  version");
    println!("  config      path [--json] | dir");
    println!("  help");
    Ok(())
}

fn load_client() -> Result<ApiClient> {
    let config = crate::config::CliConfig::load()?;
    ApiClient::new(config.server_url, config.api_token)
}

fn format_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| format!("{value}"))
}

// ── Doctor ──────────────────────────────────────────────────────────

fn cmd_doctor(args: &[String]) -> Result<()> {
    let json = args.contains(&"--json".to_string());
    let config = crate::config::CliConfig::load()?;
    checks::run_doctor(&config, json)
}

// ── Configure ───────────────────────────────────────────────────────

fn cmd_configure(args: &[String]) -> Result<()> {
    let mut server_url = None;
    let mut api_token = None;
    let mut config_path = None;
    let mut i = 0;
    while i < args.len() {
        let flag = &args[i];
        let val = args
            .get(i + 1)
            .ok_or_else(|| anyhow::anyhow!("missing value for {flag}"))?;
        match flag.as_str() {
            "--server-url" => server_url = Some(val.clone()),
            "--api-token" => api_token = Some(val.clone()),
            "--config" => config_path = Some(PathBuf::from(val)),
            _ => bail!("unknown configure option '{flag}'"),
        }
        i += 2;
    }
    let path = resolve_config_path(config_path)
        .ok_or_else(|| anyhow::anyhow!("unable to determine a config path; pass --config"))?;
    let url = server_url.unwrap_or_else(|| "http://localhost:3100".to_owned());
    let allowed_hostnames = crate::config::CliConfig::load_from(Some(path.clone()))
        .map(|config| config.allowed_hostnames)
        .unwrap_or_default();
    let config = crate::config::CliConfig {
        server_url: url,
        api_token,
        allowed_hostnames,
        config_path: Some(path.clone()),
    };
    config.save()?;
    println!("configuration saved to {}", path.display());
    Ok(())
}


// ── Config ──────────────────────────────────────────────────────────

fn cmd_config(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("path");
    match sub {
        "path" => {
            let json = args.contains(&"--json".to_string());
            let path = resolve_config_path(None);
            match path {
                Some(p) => {
                    if json {
                        println!("{}", serde_json::json!({"path": p.display().to_string()}));
                    } else {
                        println!("{}", p.display());
                    }
                }
                None => bail!("no config path available; set PARROT_CONFIG or use --config"),
            }
        }
        "dir" => {
            let dir = crate::config::default_config_path()
                .map(|p| p.parent().unwrap_or(p.as_path()).to_path_buf());
            match dir {
                Some(d) => println!("{}", d.display()),
                None => bail!("no default config directory available"),
            }
        }
        _ => {
            println!("Usage: parrot config path [--json] | dir");
            println!();
            println!("  path    Show resolved config file path");
            println!("  dir     Show config directory path");
            println!();
            println!("Options:");
            println!("  --json  Output JSON instead of plain text");
        }
    }
    Ok(())
}

// ── Environment / channels ─────────────────────────────────────────

fn cmd_env(args: &[String]) -> Result<()> {
    let config = crate::config::CliConfig::load()?;
    let environment_keys = [
        "DATABASE_URL",
        "PARROT_SERVER_URL",
        "PARROT_API_TOKEN",
        "PARROT_ALLOWED_HOSTNAMES",
        "PARROT_TELEMETRY_DISABLED",
        "DEPLOYMENT_MODE",
        "DEPLOYMENT_EXPOSURE",
        "HEARTBEAT_SCHEDULER_ENABLED",
        "HEARTBEAT_SCHEDULER_INTERVAL_MS",
    ];
    let env_state = environment_keys
        .iter()
        .map(|key| {
            (
                (*key).to_owned(),
                serde_json::json!({
                    "set": std::env::var_os(key).is_some(),
                    "value": if *key == "PARROT_API_TOKEN" { serde_json::Value::Null } else { std::env::var(key).ok().map(serde_json::Value::String).unwrap_or(serde_json::Value::Null) },
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let payload = serde_json::json!({
        "configPath": config.config_path.as_ref().map(|path| path.display().to_string()),
        "serverUrl": config.server_url,
        "apiTokenConfigured": config.api_token.is_some(),
        "allowedHostnames": config.allowed_hostnames,
        "contextPath": crate::context::resolve_context_path(None).map(|path| path.display().to_string()),
        "environment": env_state,
    });
    if args.iter().any(|arg| arg == "--json") {
        println!("{}", format_json(&payload));
    } else {
        println!("config: {}", payload["configPath"].as_str().unwrap_or("<none>"));
        println!("server: {}", payload["serverUrl"].as_str().unwrap_or_default());
        println!("api token: {}", if config.api_token.is_some() { "configured" } else { "missing" });
        println!("context: {}", payload["contextPath"].as_str().unwrap_or("<none>"));
        println!("allowed hostnames: {}", config.allowed_hostnames.join(", "));
        println!("deployment variables:");
        for key in environment_keys {
            let set = payload["environment"][key]["set"].as_bool().unwrap_or(false);
            println!("  {key}: {}", if set { "set" } else { "missing" });
        }
    }
    Ok(())
}

fn cmd_channels(args: &[String]) -> Result<()> {
    let current = crate::install_store::read_install_manifest(
        &crate::install_store::InstallStorePaths::new(),
    )?
    .map(|manifest| manifest.current.channel.to_string())
    .unwrap_or_else(|| "latest".to_owned());
    let payload = serde_json::json!({
        "current": current,
        "channels": [
            {"name": "latest", "description": "Stable releases"},
            {"name": "canary", "description": "Pre-release builds"},
        ],
    });
    if args.iter().any(|arg| arg == "--json") {
        println!("{}", format_json(&payload));
    } else {
        println!("current channel: {}", payload["current"].as_str().unwrap_or("latest"));
        println!("  latest  stable releases");
        println!("  canary  pre-release builds");
    }
    Ok(())
}

// ── Context / connection ──────────────────────────────────────────

fn cmd_context(args: &[String]) -> Result<()> {
    let explicit_path = get_flag_value(args, "--context").map(PathBuf::from);
    let (path, mut store) = crate::context::load(explicit_path)?;
    let sub = args.first().map(String::as_str).unwrap_or("show");
    match sub {
        "show" => {
            let requested_profile = get_flag_value(args, "--profile");
            let (name, profile) = crate::context::active_profile(
                &store,
                requested_profile.as_deref(),
            );
            let profile_json = serde_json::to_value(profile)?;
            let payload = serde_json::json!({
                "contextPath": path,
                "currentProfile": store.current_profile,
                "profileName": name,
                "profile": profile_json,
                "profiles": store.profiles,
            });
            if args.iter().any(|arg| arg == "--json") {
                println!("{}", format_json(&payload));
            } else {
                println!("context: {}", payload["contextPath"].as_str().unwrap_or_default());
                println!("current profile: {name}");
                println!("{}", format_json(&serde_json::to_value(profile)?));
            }
        }
        "list" | "ls" => {
            let rows = store
                .profiles
                .iter()
                .map(|(name, profile)| {
                    serde_json::json!({
                        "name": name,
                        "current": name == &store.current_profile,
                        "apiBase": profile.api_base,
                        "companyId": profile.company_id,
                        "persona": profile.persona,
                        "agentId": profile.agent_id,
                    })
                })
                .collect::<Vec<_>>();
            println!("{}", format_json(&serde_json::Value::Array(rows)));
        }
        "use" => {
            let name = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot context use <profile>"))?;
            if !store.profiles.contains_key(name) {
                bail!("context profile not found: {name}");
            }
            store.current_profile = name.clone();
            crate::context::save(&path, &store)?;
            println!("active context profile: {name}");
        }
        "set" => {
            let name = get_flag_value(args, "--profile")
                .unwrap_or_else(|| store.current_profile.clone());
            let persona = get_flag_value(args, "--persona");
            if let Some(value) = persona.as_deref() {
                if !matches!(value, "board" | "agent") {
                    bail!("--persona must be board or agent");
                }
            }
            crate::context::upsert_profile(
                &mut store,
                &name,
                crate::context::ContextProfile {
                    api_base: get_flag_value(args, "--api-base"),
                    company_id: get_flag_value(args, "--company-id"),
                    persona,
                    agent_id: get_flag_value(args, "--agent-id"),
                    agent_name: get_flag_value(args, "--agent-name"),
                    api_key_env_var_name: get_flag_value(args, "--api-key-env-var-name"),
                    token_name: None,
                    token_id: None,
                    token_created_at: None,
                },
                args.iter().any(|arg| arg == "--use"),
            )?;
            crate::context::save(&path, &store)?;
            let (_, profile) = crate::context::active_profile(&store, Some(&name));
            let payload = serde_json::json!({
                "contextPath": path,
                "currentProfile": store.current_profile,
                "profileName": name,
                "profile": profile,
            });
            if args.iter().any(|arg| arg == "--json") {
                println!("{}", format_json(&payload));
            } else {
                println!("updated context profile '{}'", name);
            }
        }
        _ => println!("Usage: parrot context show | list | use <profile> | set [--profile NAME] [--api-base URL] [--company-id ID] [--persona board|agent] [--agent-id ID] [--use]"),
    }
    Ok(())
}

fn cmd_connect(args: &[String]) -> Result<()> {
    let config_path = get_flag_value(args, "--config").map(PathBuf::from);
    let path = resolve_config_path(config_path)
        .ok_or_else(|| anyhow::anyhow!("unable to determine a config path; pass --config"))?;
    let existing = crate::config::CliConfig::load_from(Some(path.clone())).unwrap_or(crate::config::CliConfig {
        server_url: "http://localhost:3100".to_owned(),
        api_token: None,
        allowed_hostnames: Vec::new(),
        config_path: Some(path.clone()),
    });
    let server_url = get_flag_value(args, "--server-url")
        .or_else(|| get_flag_value(args, "--api-base"))
        .unwrap_or_else(|| existing.server_url.clone());
    if !(server_url.starts_with("http://") || server_url.starts_with("https://")) {
        bail!("server URL must start with http:// or https://");
    }
    let supplied_token = get_flag_value(args, "--api-token")
        .or_else(|| get_flag_value(args, "--api-key"));
    let api_token = supplied_token.clone().or(existing.api_token.clone());
    if !args.iter().any(|arg| arg == "--no-check") {
        let client = ApiClient::new(server_url.clone(), api_token.clone())?;
        if !matches!(client.health_check()?, crate::services::ServiceStatus::Healthy | crate::services::ServiceStatus::Degraded) {
            bail!("server health check failed for {server_url}; use --no-check to save without probing");
        }
    }
    let saved = crate::config::CliConfig {
        server_url: server_url.clone(),
        api_token,
        allowed_hostnames: existing.allowed_hostnames,
        config_path: Some(path),
    };
    saved.save()?;

    let context_path = crate::context::resolve_context_path(
        get_flag_value(args, "--context").map(PathBuf::from),
    )
    .ok_or_else(|| anyhow::anyhow!("unable to determine context path"))?;
    let (_, mut store) = crate::context::load(Some(context_path.clone()))?;
    let profile_name = get_flag_value(args, "--profile")
        .unwrap_or_else(|| store.current_profile.clone());
    let env_name = get_flag_value(args, "--api-key-env-var-name")
        .unwrap_or_else(|| "PARROT_API_TOKEN".to_owned());
    crate::context::upsert_profile(
        &mut store,
        &profile_name,
        crate::context::ContextProfile {
            api_base: Some(server_url.clone()),
            company_id: get_flag_value(args, "--company-id"),
            persona: get_flag_value(args, "--persona"),
            agent_id: get_flag_value(args, "--agent-id"),
            agent_name: None,
            api_key_env_var_name: Some(env_name),
            token_name: None,
            token_id: None,
            token_created_at: None,
        },
        true,
    )?;
    crate::context::save(&context_path, &store)?;
    println!("connected profile '{}' to {}", profile_name, server_url);
    Ok(())
}

fn load_client_for_args(args: &[String]) -> Result<ApiClient> {
    let config_path = get_flag_value(args, "--config").map(PathBuf::from);
    let config = crate::config::CliConfig::load_from(config_path)?;
    let context_path = get_flag_value(args, "--context").map(PathBuf::from);
    let (_, store) = crate::context::load(context_path)?;
    let requested_profile = get_flag_value(args, "--profile");
    let (_, profile) = crate::context::active_profile(
        &store,
        requested_profile.as_deref(),
    );
    let base = get_flag_value(args, "--api-base")
        .or_else(|| get_flag_value(args, "--server-url"))
        .or_else(|| profile.api_base.clone())
        .unwrap_or(config.server_url);
    let token = get_flag_value(args, "--api-key")
        .or_else(|| get_flag_value(args, "--api-token"))
        .or_else(|| crate::context::profile_token(profile))
        .or(config.api_token);
    ApiClient::new(base, token)
}

// ── Allowed hostnames ──────────────────────────────────────────────

fn cmd_allowed_hostname(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    let config_path = get_flag_value(args, "--config").map(PathBuf::from);
    let path = resolve_config_path(config_path)
        .ok_or_else(|| anyhow::anyhow!("unable to determine a config path; pass --config"))?;
    let mut config = crate::config::CliConfig::load_from(Some(path.clone()))?;
    match sub {
        "add" => {
            let host = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot allowed-hostname add <host>"))?;
            let normalized = normalize_hostname(host)?;
            if !config.allowed_hostnames.iter().any(|value| value == &normalized) {
                config.allowed_hostnames.push(normalized.clone());
                config.allowed_hostnames.sort();
                config.save()?;
            }
            if args.iter().any(|arg| arg == "--json") {
                println!("{}", serde_json::json!({"added": normalized, "allowedHostnames": config.allowed_hostnames}));
            } else {
                println!("allowed hostname: {}", normalized);
                println!("restart the server for the setting to take effect");
            }
        }
        "list" | "ls" => {
            if args.iter().any(|arg| arg == "--json") {
                println!("{}", serde_json::json!({"allowedHostnames": config.allowed_hostnames}));
            } else if config.allowed_hostnames.is_empty() {
                println!("no allowed hostnames configured");
            } else {
                for hostname in config.allowed_hostnames {
                    println!("{hostname}");
                }
            }
        }
        _ => println!("Usage: parrot allowed-hostname add <host> | list [--json]"),
    }
    Ok(())
}

fn normalize_hostname(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        bail!("hostname must not be empty or contain whitespace");
    }
    let parsed = if value.starts_with("http://") || value.starts_with("https://") {
        reqwest::Url::parse(value)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
    } else {
        Some(value.trim_end_matches('/').trim_end_matches('.').to_owned())
    };
    let hostname = parsed
        .filter(|host| !host.is_empty() && !host.contains('/'))
        .ok_or_else(|| anyhow::anyhow!("invalid hostname: {value}"))?;
    Ok(hostname.to_ascii_lowercase())
}

// ── Tokens ──────────────────────────────────────────────────────────

fn cmd_token(args: &[String]) -> Result<()> {
    let kind = args.first().map(String::as_str).unwrap_or("help");
    let action = args.get(1).map(String::as_str).unwrap_or("help");
    let client = load_client_for_args(args)?;
    match (kind, action) {
        ("agent", "create") => {
            let agent_id = get_flag_value_any(args, &["--agent-id", "--agent"])
                .or_else(|| args.get(2).cloned())
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot token agent create <agentId> [--name NAME]"))?;
            let name = get_flag_value(args, "--name").unwrap_or_else(|| "parrot-agent".to_owned());
            let result = client.create_agent_key(&agent_id, &serde_json::json!({"name": name}))?;
            println!("{}", format_json(&result));
        }
        ("agent", "list") => {
            let agent_id = get_flag_value_any(args, &["--agent-id", "--agent"])
                .or_else(|| args.get(2).cloned())
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot token agent list <agentId>"))?;
            println!("{}", format_json(&client.list_agent_keys(&agent_id)?));
        }
        ("agent", "revoke") => {
            let agent_id = get_flag_value_any(args, &["--agent-id", "--agent"])
                .or_else(|| args.get(2).cloned())
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot token agent revoke <agentId> <keyId>"))?;
            let key_id = get_flag_value(args, "--key-id")
                .or_else(|| args.get(3).cloned())
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot token agent revoke <agentId> <keyId>"))?;
            println!("{}", format_json(&client.revoke_agent_key(&agent_id, &key_id)?));
        }
        ("board", "create") => {
            let mut body = serde_json::json!({
                "name": get_flag_value(args, "--name").unwrap_or_else(|| "parrot-board".to_owned()),
            });
            if let Some(company_id) = get_flag_value(args, "--company-id") {
                body["requestedCompanyId"] = serde_json::Value::String(company_id);
            }
            println!("{}", format_json(&client.create_board_api_key(&body)?));
        }
        ("board", "list") => println!("{}", format_json(&client.list_board_api_keys()?)),
        ("board", "revoke") => {
            let key_id = get_flag_value(args, "--key-id")
                .or_else(|| args.get(2).cloned())
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot token board revoke <keyId>"))?;
            println!("{}", format_json(&client.revoke_board_api_key(&key_id)?));
        }
        _ => println!("Usage: parrot token agent create|list|revoke <agentId> [<keyId>] | board create|list|revoke [<keyId>]"),
    }
    Ok(())
}

// ── Agent prompt handoff ───────────────────────────────────────────

fn cmd_prompt(args: &[String]) -> Result<()> {
    let client = load_client_for_args(args)?;
    let prompt = collect_prompt_text(args);
    if prompt.trim().is_empty() {
        bail!("prompt text is required");
    }
    let wake = !args.iter().any(|arg| arg == "--no-wake");

    let mut agent = None;
    let mut agent_id = get_flag_value_any(args, &["--agent-id", "--agent"]);
    if agent_id.is_none() {
        if let Ok(current) = client.get_current_agent() {
            agent_id = current
                .get("id")
                .and_then(|value| value.as_str())
                .map(str::to_owned);
            agent = Some(current);
        }
    }
    let agent_id = agent_id.ok_or_else(|| {
        anyhow::anyhow!("an agent is required; pass --agent-id or use an agent API token")
    })?;
    if agent.is_none() {
        let company_id = get_flag_value(args, "--company-id")
            .ok_or_else(|| anyhow::anyhow!("--company-id is required when using a board token"))?;
        agent = Some(client.get_agent(&company_id, &agent_id)?);
    }
    let agent = agent.unwrap_or_else(|| serde_json::json!({}));
    let company_id = get_flag_value(args, "--company-id")
        .or_else(|| agent.get("companyId").and_then(|value| value.as_str()).map(str::to_owned))
        .ok_or_else(|| anyhow::anyhow!("company id is unavailable; pass --company-id"))?;
    let issue_id = get_flag_value_any(args, &["--issue", "--issue-id"]);
    let title = get_flag_value(args, "--title").unwrap_or_else(|| prompt_title(&prompt));

    let (mode, issue_or_comment) = if let Some(issue_id) = issue_id.as_deref() {
        let comment = client.add_issue_comment(
            issue_id,
            &serde_json::json!({"body": prompt, "resume": wake}),
        )?;
        ("comment", comment)
    } else {
        let issue = client.create_issue(
            &company_id,
            &serde_json::json!({
                "title": title,
                "description": prompt,
                "status": "todo",
                "priority": "medium",
                "assigneeAgentId": agent_id,
            }),
        )?;
        ("issue", issue)
    };

    let wakeup = if wake {
        let target_issue_id = issue_or_comment
            .get("id")
            .and_then(|value| value.as_str())
            .or(issue_id.as_deref())
            .ok_or_else(|| anyhow::anyhow!("API response did not include an issue id"))?;
        Some(client.wakeup_agent(
            &agent_id,
            &serde_json::json!({
                "source": "on_demand",
                "triggerDetail": "manual",
                "reason": "cli_prompt_handoff",
                "payload": {"issueId": target_issue_id},
            }),
        )?)
    } else {
        None
    };
    println!(
        "{}",
        format_json(&serde_json::json!({
            "ok": true,
            "mode": mode,
            "companyId": company_id,
            "agent": agent,
            "work": issue_or_comment,
            "wakeup": wakeup,
        }))
    );
    Ok(())
}

fn collect_prompt_text(args: &[String]) -> String {
    let value_flags = [
        "--agent-id", "--agent", "--company-id", "--issue", "--issue-id", "--title",
        "--api-base", "--api-key", "--api-token", "--config", "--context", "--profile",
        "--payload-json", "--context-json", "--reason",
    ];
    let mut words = Vec::new();
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if value_flags.contains(&arg.as_str()) {
            skip_next = true;
            continue;
        }
        if arg.starts_with('-') || arg == "--no-wake" || arg == "--json" {
            continue;
        }
        words.push(arg.as_str());
    }
    words.join(" ")
}

fn prompt_title(prompt: &str) -> String {
    let line = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Prompt handoff");
    if line.chars().count() <= 100 {
        line.to_owned()
    } else {
        let short = line.chars().take(97).collect::<String>();
        format!("{short}...")
    }
}

// ── Heartbeat runner ───────────────────────────────────────────────

fn cmd_heartbeat(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    if sub != "run" {
        println!("Usage: parrot heartbeat run --agent-id ID [--source SOURCE] [--trigger TRIGGER] [--timeout-ms N]");
        return Ok(());
    }
    let agent_id = get_flag_value_any(args, &["--agent-id", "-a"])
        .or_else(|| {
            crate::context::load(get_flag_value(args, "--context").map(PathBuf::from))
                .ok()
                .and_then(|(_, store)| {
                    crate::context::active_profile(
                        &store,
                        get_flag_value(args, "--profile").as_deref(),
                    )
                    .1
                    .agent_id
                    .clone()
                })
        })
        .ok_or_else(|| anyhow::anyhow!("Usage: parrot heartbeat run --agent-id ID"))?;
    let source = get_flag_value(args, "--source").unwrap_or_else(|| "on_demand".to_owned());
    let trigger = get_flag_value(args, "--trigger").unwrap_or_else(|| "manual".to_owned());
    let mut body = serde_json::json!({
        "source": source,
        "triggerDetail": trigger,
        "reason": get_flag_value(args, "--reason").unwrap_or_else(|| "cli_heartbeat_run".to_owned()),
    });
    if let Some(value) = get_flag_value(args, "--payload-json") {
        body["payload"] = serde_json::from_str(&value).context("parse --payload-json")?;
    }
    if let Some(value) = get_flag_value(args, "--context-json") {
        body["contextSnapshot"] = serde_json::from_str(&value).context("parse --context-json")?;
    }
    if let Some(value) = get_flag_value(args, "--idempotency-key") {
        body["idempotencyKey"] = serde_json::Value::String(value);
    }
    if args.iter().any(|arg| arg == "--force-fresh-session") {
        body["forceFreshSession"] = serde_json::Value::Bool(true);
    }
    let client = load_client_for_args(args)?;
    let invoked = client.wakeup_agent(&agent_id, &body)?;
    if invoked.get("status").and_then(|value| value.as_str()) == Some("skipped") {
        println!("{}", format_json(&invoked));
        return Ok(());
    }
    let run_id = invoked
        .get("id")
        .or_else(|| invoked.get("runId"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("heartbeat wakeup response did not contain a run id"))?
        .to_owned();
    let json_output = args.iter().any(|arg| arg == "--json");
    if !json_output {
        println!("heartbeat run: {run_id}");
    }
    let timeout_ms = get_flag_value(args, "--timeout-ms")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let deadline = (timeout_ms > 0)
        .then(|| std::time::Instant::now() + Duration::from_millis(timeout_ms));
    let mut after_seq = 0_i64;
    loop {
        let events = client.list_run_events(&run_id, after_seq, 100)?;
        if let Some(rows) = events.as_array() {
            for event in rows {
                after_seq = after_seq.max(
                    event
                        .get("seq")
                        .and_then(|value| value.as_i64())
                        .unwrap_or(after_seq),
                );
                print_heartbeat_event(event, json_output);
            }
        }
        let run = client.get_run(&run_id)?;
        let status = run
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        if matches!(status, "succeeded" | "failed" | "cancelled" | "timed_out") {
            if json_output {
                println!("{}", format_json(&run));
            } else {
                println!("heartbeat status: {status}");
            }
            return Ok(());
        }
        if deadline.is_some_and(|value| std::time::Instant::now() >= value) {
            bail!("heartbeat run {run_id} timed out while status was {status}");
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn print_heartbeat_event(event: &serde_json::Value, json_output: bool) {
    if json_output {
        println!("{}", format_json(event));
        return;
    }
    let event_type = event
        .get("eventType")
        .or_else(|| event.get("type"))
        .and_then(|value| value.as_str())
        .unwrap_or("heartbeat.run.event");
    if event_type == "heartbeat.run.log" {
        let payload = event.get("payload").cloned().unwrap_or_default();
        let stream = payload
            .get("stream")
            .and_then(|value| value.as_str())
            .unwrap_or("system");
        let chunk = payload
            .get("chunk")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if stream == "stderr" {
            eprint!("[stderr] {chunk}");
        } else {
            print!("[{stream}] {chunk}");
        }
        let _ = io::stdout().flush();
    } else if let Some(message) = event.get("message").and_then(|value| value.as_str()) {
        println!("[{event_type}] {message}");
    } else {
        println!(
            "[{event_type}] {}",
            event.get("payload").unwrap_or(&serde_json::Value::Null)
        );
    }
}

// ── Export bundle alias ─────────────────────────────────────────────

fn cmd_zip(args: &[String]) -> Result<()> {
    let company_id = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("Usage: parrot zip <companyId> [--output FILE]"))?;
    let client = load_client_for_args(args)?;
    let bundle = client.export_company(company_id)?;
    if let Some(output) = get_flag_value(args, "--output") {
        fs::write(&output, serde_json::to_vec_pretty(&bundle)?)
            .with_context(|| format!("failed to write export bundle {output}"))?;
        println!("export bundle written to {output}");
    } else {
        println!("{}", format_json(&bundle));
    }
    Ok(())
}

// ── Service ───────────────────────────────────────────────────────────
// ── Service ───────────────────────────────────────────────────────────

fn cmd_service(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    match sub {
        "status" => {
            let client = load_client();
            match client {
                Ok(c) => match c.health_check() {
                    Ok(s) => println!("server: {:?}", s),
                    Err(e) => println!("server: unhealthy ({e})"),
                },
                Err(e) => println!("config: no valid configuration ({e})"),
            }
            #[cfg(target_family = "unix")]
            {
                let svc = std::process::Command::new("systemctl")
                    .args(["is-active", "parrot"])
                    .output();
                match svc {
                    Ok(out) => println!("systemd: {}", String::from_utf8_lossy(&out.stdout).trim()),
                    Err(_) => println!("systemd: not checked (systemctl not available)"),
                }
                // Check for unit file and show install hint if missing
                let unit_path = std::path::PathBuf::from("/etc/systemd/system/parrot.service");
                if !unit_path.exists() {
                    let user_unit = std::path::PathBuf::from(
                        std::env::var("HOME").unwrap_or_default()
                            + "/.config/systemd/user/parrot.service",
                    );
                    if !user_unit.exists() {
                        println!(
                            "hint: no systemd unit file found; run `parrot install --install-service` to create one"
                        );
                    }
                }
            }
            #[cfg(target_family = "windows")]
            {
                let svc = std::process::Command::new("sc")
                    .args(["query", "parrot"])
                    .output();
                match svc {
                    Ok(out) => {
                        let text = String::from_utf8_lossy(&out.stdout);
                        if text.contains("RUNNING") {
                            println!("windows service: running");
                        } else if text.contains("STOPPED") {
                            println!("windows service: stopped");
                        } else {
                            println!("windows service: not found");
                        }
                    }
                    Err(_) => println!("windows service: not checked"),
                }
            }
            Ok(())
        }
        "start" => {
            #[cfg(target_family = "unix")]
            {
                let status = std::process::Command::new("systemctl")
                    .args(["start", "parrot"])
                    .status()?;
                if status.success() {
                    println!("started parrot service");
                } else {
                    bail!("failed to start parrot service");
                }
            }
            #[cfg(not(target_family = "unix"))]
            bail!("service start is only supported on Linux with systemd");
            Ok(())
        }
        "stop" => {
            #[cfg(target_family = "unix")]
            {
                let status = std::process::Command::new("systemctl")
                    .args(["stop", "parrot"])
                    .status()?;
                if status.success() {
                    println!("stopped parrot service");
                } else {
                    bail!("failed to stop parrot service");
                }
            }
            #[cfg(not(target_family = "unix"))]
            bail!("service stop is only supported on Linux with systemd");
            Ok(())
        }
        "restart" => {
            #[cfg(target_family = "unix")]
            {
                let status = std::process::Command::new("systemctl")
                    .args(["restart", "parrot"])
                    .status()?;
                if status.success() {
                    println!("restarted parrot service");
                } else {
                    bail!("failed to restart parrot service");
                }
            }
            #[cfg(not(target_family = "unix"))]
            bail!("service restart is only supported on Linux with systemd");
            Ok(())
        }
        "log" => {
            let tail_lines: usize = args.get(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(50);

            #[cfg(target_family = "unix")]
            {
                // Try systemd journal first
                let journal = std::process::Command::new("journalctl")
                    .args(["-u", "parrot", "--no-pager", "-n", &tail_lines.to_string(), "-f"])
                    .output();

                match journal {
                    Ok(out) if out.status.success() => {
                        print!("{}", String::from_utf8_lossy(&out.stdout));
                    }
                    Ok(_) | Err(_) => {
                        // Fallback to log file
                        let log_path = std::path::PathBuf::from(
                            std::env::var("PARROT_LOG_DIR").unwrap_or_else(|_| "/var/log/parrot".to_string())
                        ).join("server.log");

                        if log_path.exists() {
                            let content = std::fs::read_to_string(&log_path)?;
                            let lines: Vec<&str> = content.lines().collect();
                            let start = lines.len().saturating_sub(tail_lines);
                            for line in &lines[start..] {
                                println!("{line}");
                            }
                        } else {
                            println!("no log file found at {}", log_path.display());
                            println!("hint: ensure PARROT_LOG_DIR is set or service is running under systemd");
                        }
                    }
                }
            }

            #[cfg(not(target_family = "unix"))]
            {
                let log_path = std::path::PathBuf::from(
                    std::env::var("PARROT_LOG_DIR").unwrap_or_else(|_| {
                        std::env::var("APPDATA").unwrap_or_default() + "\\parrot\\logs"
                    })
                ).join("server.log");

                if log_path.exists() {
                    let content = std::fs::read_to_string(&log_path)?;
                    let lines: Vec<&str> = content.lines().collect();
                    let start = lines.len().saturating_sub(tail_lines);
                    for line in &lines[start..] {
                        println!("{line}");
                    }
                } else {
                    println!("no log file found at {}", log_path.display());
                }
            }

            Ok(())
        }
        _ => {
            println!("Usage: parrot service status | start | stop | restart | log [LINES]");
            Ok(())
        }
    }
}

// ── Install ───────────────────────────────────────────────────────────

fn cmd_install(args: &[String]) -> Result<()> {
    let install_dir = get_flag_value(args, "--dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            #[cfg(target_family = "unix")]
            {
                PathBuf::from("/usr/local/bin")
            }
            #[cfg(target_family = "windows")]
            {
                PathBuf::from(
                    std::env::var("PROGRAMFILES")
                        .unwrap_or_else(|_| "C:\\Program Files\\parrot".into()),
                )
            }
        });

    let install_service = args.contains(&"--install-service".to_string());
    let service_dir = get_flag_value(args, "--service-dir")
        .map(PathBuf::from)
        .or_else(|| {
            #[cfg(target_family = "unix")]
            {
                if unsafe { libc::geteuid() } == 0 {
                    Some(PathBuf::from("/etc/systemd/system"))
                } else {
                    let home = std::env::var("HOME").unwrap_or_default();
                    Some(PathBuf::from(home).join(".config/systemd/user"))
                }
            }
            #[cfg(not(target_family = "unix"))]
            {
                None
            }
        });

    let self_path = std::env::current_exe()?;
    let target_path = install_dir.join("parrot");
    #[cfg(target_family = "unix")]
    {
        std::fs::create_dir_all(&install_dir)?;
        std::fs::copy(&self_path, &target_path)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&target_path, std::fs::Permissions::from_mode(0o755))?;
        println!("installed parrot to {}", target_path.display());

        if install_service {
            let service_dir = service_dir.ok_or_else(|| {
                anyhow::anyhow!(
                    "--service-dir required on non-Linux platforms; use --service-dir to specify"
                )
            })?;
            std::fs::create_dir_all(&service_dir)?;
            let unit_path = service_dir.join("parrot.service");
            let executable = target_path.canonicalize().unwrap_or(target_path.clone());
            let unit_content = format!(
                "[Unit]\n\
                 Description=Parrot Agent Server\n\
                 After=network.target\n\n\
                 [Service]\n\
                 Type=notify\n\
                 ExecStart={}\n\
                 NotifyAccess=main\n\
                 Restart=on-failure\n\
                 RestartSec=5s\n\n\
                 [Install]\n\
                 WantedBy=default.target\n",
                executable.display()
            );
            std::fs::write(&unit_path, unit_content)?;
            println!("installed systemd unit to {}", unit_path.display());
            println!("run: systemctl daemon-reload");
            println!("run: systemctl enable --now parrot (or --user for user service)");
        }
    }
    #[cfg(target_family = "windows")]
    {
        std::fs::create_dir_all(&install_dir)?;
        std::fs::copy(&self_path, &target_path)?;
        if install_service {
            println!(
                "warning: --install-service is not supported on Windows; skip systemd unit generation"
            );
        }
        println!("installed parrot to {}", target_path.display());
    }
    #[cfg(not(any(target_family = "unix", target_family = "windows")))]
    bail!("install is not supported on this platform");

    Ok(())
}

// ── Auth ─────────────────────────────────────────────────────────────

fn cmd_auth(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    match sub {
        "get-session" | "session" | "status" => {
            let client = load_client()?;
            let session = client.get_session()?;
            println!("{}", format_json(&session));
            Ok(())
        }
        "bootstrap-ceo" => cmd_bootstrap_ceo(&args[1..]),
        _ => {
            println!("Usage: parrot auth get-session | bootstrap-ceo [--db-url URL] [--base-url URL] [--force]");
            Ok(())
        }
    }
}

/// Create a one-time instance bootstrap invite, matching Paperclip's
/// `auth bootstrap-ceo` command.  This is intentionally a local database
/// operation: before the first board user exists there is no bearer identity
/// that could authorize an HTTP request.
fn cmd_bootstrap_ceo(args: &[String]) -> Result<()> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("Usage: parrot auth bootstrap-ceo [--db-url URL] [--base-url URL] [--expires-hours N] [--force] [--json]");
        return Ok(());
    }
    let db_url = get_flag_value(args, "--db-url")
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .or_else(|| std::env::var("PARROT_DATABASE_URL").ok())
        .ok_or_else(|| anyhow::anyhow!("database URL is required; pass --db-url or set DATABASE_URL"))?;
    let base_url = get_flag_value(args, "--base-url")
        .or_else(|| std::env::var("PARROT_PUBLIC_URL").ok())
        .or_else(|| std::env::var("PAPERCLIP_PUBLIC_URL").ok())
        .or_else(|| crate::config::CliConfig::load().ok().map(|config| config.server_url))
        .unwrap_or_else(|| "http://localhost:3100".to_owned())
        .trim_end_matches('/')
        .to_owned();
    let force = args.iter().any(|arg| arg == "--force");
    let expires_hours = get_flag_value(args, "--expires-hours")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(72)
        .clamp(1, 24 * 30);
    let json_output = args.iter().any(|arg| arg == "--json");
    let runtime = tokio::runtime::Runtime::new().context("create bootstrap runtime")?;
    let result = runtime.block_on(async move {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&db_url)
            .await
            .context("connect to database for bootstrap invite")?;
        let admin_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM instance_user_roles WHERE role = 'instance_admin'",
        )
        .fetch_one(&pool)
        .await
        .context("check instance administrators")?;
        if admin_count > 0 && !force {
            anyhow::bail!("instance already has an admin; pass --force to create another invite");
        }
        if force {
            sqlx::query(
                "UPDATE invites
                 SET revoked_at = NOW()
                 WHERE invite_type = 'bootstrap_ceo'
                   AND revoked_at IS NULL AND accepted = false AND expires_at > NOW()",
            )
            .execute(&pool)
            .await
            .context("revoke existing bootstrap invites")?;
        }
        let token = format!("pcp_bootstrap_{}", uuid::Uuid::new_v4().simple());
        let expires_at = chrono::Utc::now() + chrono::Duration::hours(expires_hours);
        let invite_id: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO invites
                (id, company_id, invite_type, invited_by_user_id, token,
                 allowed_join_types, expires_at, accepted, created_at)
             VALUES ($1, NULL, 'bootstrap_ceo'::invite_type, NULL, $2,
                     'human'::allowed_join_types, $3, false, NOW())
             RETURNING id",
        )
        .bind(uuid::Uuid::new_v4())
        .bind(&token)
        .bind(expires_at)
        .fetch_one(&pool)
        .await
        .context("create bootstrap CEO invite")?;
        pool.close().await;
        Ok::<_, anyhow::Error>(serde_json::json!({
            "id": invite_id,
            "token": token,
            "inviteUrl": format!("{base_url}/invite/{token}"),
            "expiresAt": expires_at,
            "inviteType": "bootstrap_ceo",
        }))
    })?;
    if json_output {
        println!("{}", format_json(&result));
    } else {
        println!("created bootstrap CEO invite");
        println!("invite URL: {}", result["inviteUrl"].as_str().unwrap_or_default());
        println!("expires: {}", result["expiresAt"]);
    }
    Ok(())
}


fn cmd_company(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let client = load_client()?;
    match sub {
        "list" | "ls" => {
            let companies = client.list_companies()?;
            println!("{}", format_json(&companies));
            Ok(())
        }
        "get" => {
            let id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot company get <id>"))?;
            let company = client.get_company(id)?;
            println!("{}", format_json(&company));
            Ok(())
        }
        "create" => {
            let raw = get_flag_value(args, "--json")
                .ok_or_else(|| anyhow::anyhow!("company create requires --json '{{...}}'"))?;
            let body: serde_json::Value = serde_json::from_str(&raw)?;
            println!("{}", format_json(&client.create_company(&body)?));
            Ok(())
        }
        "delete" => {
            let id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot company delete <id>"))?;
            println!("{}", format_json(&client.delete_company(id)?));
            Ok(())
        }
        "export" => {
            let id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot company export <id>"))?;
            println!("{}", format_json(&client.export_company(id)?));
            Ok(())
        }
        "import" => {
            let raw = get_flag_value(args, "--json")
                .ok_or_else(|| anyhow::anyhow!("company import requires --json '{{...}}'"))?;
            let body: serde_json::Value = serde_json::from_str(&raw)?;
            println!("{}", format_json(&client.import_company(&body)?));
            Ok(())
        }
        _ => {
            println!("Usage: parrot company list | get <id> | create --json '{{...}}' | delete <id> | export <id> | import --json '{{...}}'");
            Ok(())
        }
    }
}

// ── Approval ───────────────────────────────────────────────────────

fn cmd_approval(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let client = load_client()?;
    match sub {
        "list" | "ls" => {
            let company_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot approval list <companyId>"))?;
            println!("{}", format_json(&client.list_approvals(company_id)?));
            Ok(())
        }
        "get" => {
            let id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot approval get <id>"))?;
            println!("{}", format_json(&client.get_approval(id)?));
            Ok(())
        }
        "approve" => {
            let id = args.get(1).ok_or_else(|| {
                anyhow::anyhow!("Usage: parrot approval approve <id> [--json '{{...}}']")
            })?;
            let body = approval_body(args);
            println!("{}", format_json(&client.approve_approval(id, &body)?));
            Ok(())
        }
        "reject" => {
            let id = args.get(1).ok_or_else(|| {
                anyhow::anyhow!("Usage: parrot approval reject <id> [--json '{{...}}']")
            })?;
            let body = approval_body(args);
            println!("{}", format_json(&client.reject_approval(id, &body)?));
            Ok(())
        }
        "resubmit" => {
            let id = args.get(1).ok_or_else(|| {
                anyhow::anyhow!("Usage: parrot approval resubmit <id> [--json '{{...}}']")
            })?;
            let body = get_flag_value(args, "--json")
                .map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::Null));
            println!(
                "{}",
                format_json(&client.resubmit_approval(id, body.as_ref())?)
            );
            Ok(())
        }
        _ => {
            println!("Usage: parrot approval list <companyId> | get <id> | approve <id> [--json] | reject <id> [--json] | resubmit <id> [--json]");
            Ok(())
        }
    }
}

fn approval_body(args: &[String]) -> serde_json::Value {
    match get_flag_value(args, "--json") {
        Some(s) => serde_json::from_str(&s).unwrap_or(serde_json::json!({})),
        None => serde_json::json!({}),
    }
}

// ── Pipeline ───────────────────────────────────────────────────────

fn cmd_pipeline(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let client = load_client()?;
    match sub {
        "list" | "ls" => {
            let company_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot pipeline list <companyId>"))?;
            println!("{}", format_json(&client.list_pipelines(company_id)?));
            Ok(())
        }
        "get" => {
            let id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot pipeline get <id>"))?;
            println!("{}", format_json(&client.get_pipeline(id)?));
            Ok(())
        }
        _ => {
            println!("Usage: parrot pipeline list <companyId> | get <id>");
            Ok(())
        }
    }
}

// ── Skill ──────────────────────────────────────────────────────────

fn cmd_skill(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let client = load_client()?;
    match sub {
        "list" | "ls" => {
            println!("{}", format_json(&client.list_skills()?));
            Ok(())
        }
        "get" => {
            let name = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot skill get <name>"))?;
            println!("{}", format_json(&client.get_skill(name)?));
            Ok(())
        }
        _ => {
            println!("Usage: parrot skill list | get <name>");
            Ok(())
        }
    }
}

// ── Team ───────────────────────────────────────────────────────────

fn cmd_team(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("catalog");
    let client = load_client()?;
    match sub {
        "catalog" => {
            println!("{}", format_json(&client.list_teams_catalog()?));
            Ok(())
        }
        _ => {
            println!("Usage: parrot team catalog");
            Ok(())
        }
    }
}

// ── Plugin ─────────────────────────────────────────────────────────

fn cmd_plugin(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    match sub {
        "create" => {
            // Offline scaffolding: `parrot plugin create <name> --output <dir>
            // [--template default|connector|workspace|environment]`
            // (Paperclip create-paperclip-plugin equivalent). No API client needed.
            let name = args.get(1).ok_or_else(|| {
                anyhow::anyhow!("Usage: parrot plugin create <name> --output <dir> [--template T]")
            })?;
            let output = get_flag_value(args, "--output")
                .ok_or_else(|| anyhow::anyhow!("plugin create requires --output <dir>"))?;
            let options = crate::plugin_scaffold::ScaffoldPluginOptions {
                plugin_name: name.clone(),
                output_dir: output.clone(),
                template: get_flag_value(args, "--template"),
                display_name: get_flag_value(args, "--display-name"),
                description: get_flag_value(args, "--description"),
                author: get_flag_value(args, "--author"),
                category: get_flag_value(args, "--category"),
            };
            let written = crate::plugin_scaffold::scaffold_plugin_project(&options)
                .map_err(|message| anyhow::anyhow!("{message}"))?;
            println!("Created plugin scaffold at {output}");
            for path in &written {
                println!("  {path}");
            }
            Ok(())
        }
        _ => {
            // Networked subcommands require the API client.
            let client = load_client()?;
            match sub {
                "list" | "ls" => {
                    println!("{}", format_json(&client.list_plugins()?));
                    Ok(())
                }
                "install" => {
                    let raw = get_flag_value(args, "--json").ok_or_else(|| {
                        anyhow::anyhow!("plugin install requires --json '{{...}}'")
                    })?;
                    let body: serde_json::Value = serde_json::from_str(&raw)?;
                    println!("{}", format_json(&client.install_plugin(&body)?));
                    Ok(())
                }
                "enable" => {
                    let id = args
                        .get(1)
                        .ok_or_else(|| anyhow::anyhow!("Usage: parrot plugin enable <id>"))?;
                    println!("{}", format_json(&client.enable_plugin(id)?));
                    Ok(())
                }
                "disable" => {
                    let id = args
                        .get(1)
                        .ok_or_else(|| anyhow::anyhow!("Usage: parrot plugin disable <id>"))?;
                    println!("{}", format_json(&client.disable_plugin(id)?));
                    Ok(())
                }
                _ => {
                    println!("Usage: parrot plugin list | create <name> --output <dir> | install --json '{{...}}' | enable <id> | disable <id>");
                    Ok(())
                }
            }
        }
    }
}

// ── Dashboard ─────────────────────────────────────────────────────

fn cmd_dashboard(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("get");
    let client = load_client()?;
    match sub {
        "get" => {
            let company_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot dashboard get <companyId>"))?;
            println!("{}", format_json(&client.get_dashboard(company_id)?));
            Ok(())
        }
        _ => {
            println!("Usage: parrot dashboard get <companyId>");
            Ok(())
        }
    }
}

// ── Cost ───────────────────────────────────────────────────────────

fn cmd_cost(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let client = load_client()?;
    match sub {
        "summary" => {
            let company_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot cost summary <companyId>"))?;
            println!("{}", format_json(&client.get_cost_summary(company_id)?));
            Ok(())
        }
        _ => {
            println!("Usage: parrot cost summary <companyId>");
            Ok(())
        }
    }
}

// ── Feedback ──────────────────────────────────────────────────────

fn cmd_feedback(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let client = load_client()?;
    match sub {
        "get" => {
            let trace_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot feedback get <traceId>"))?;
            println!("{}", format_json(&client.get_feedback_trace(trace_id)?));
            Ok(())
        }
        _ => {
            println!("Usage: parrot feedback get <traceId>");
            Ok(())
        }
    }
}

// ── Access ────────────────────────────────────────────────────────

fn cmd_access(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let client = load_client()?;
    match sub {
        "org-chart" => {
            let company_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot access org-chart <companyId>"))?;
            println!("{}", format_json(&client.get_org_chart(company_id)?));
            Ok(())
        }
        _ => {
            println!("Usage: parrot access org-chart <companyId>");
            Ok(())
        }
    }
}

// ── Workspace ─────────────────────────────────────────────────────

fn cmd_workspace(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let client = load_client()?;
    match sub {
        "list" | "ls" => {
            let company_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot workspace list <companyId>"))?;
            println!("{}", format_json(&client.list_workspaces(company_id)?));
            Ok(())
        }
        "get" => {
            let id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot workspace get <id>"))?;
            println!("{}", format_json(&client.get_workspace(id)?));
            Ok(())
        }
        _ => {
            println!("Usage: parrot workspace list <companyId> | get <id>");
            Ok(())
        }
    }
}

// ── Run ───────────────────────────────────────────────────────────

fn cmd_run(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let client = load_client()?;
    match sub {
        "list" | "ls" => {
            let issue_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot run list <issueId>"))?;
            println!("{}", format_json(&client.list_issue_runs(issue_id)?));
            Ok(())
        }
        "get" => {
            let id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot run get <id>"))?;
            println!("{}", format_json(&client.get_run(id)?));
            Ok(())
        }
        _ => {
            println!("Usage: parrot run list <issueId> | get <id>");
            Ok(())
        }
    }
}

// ── Channel ───────────────────────────────────────────────────────

fn cmd_channel(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    let client = load_client()?;
    match sub {
        "list" | "ls" => {
            let company_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot channel list <companyId>"))?;
            println!("{}", format_json(&client.list_channels(company_id)?));
            Ok(())
        }
        _ => {
            println!("Usage: parrot channel list <companyId>");
            Ok(())
        }
    }
}

// ── Agent ─────────────────────────────────────────────────────────────

fn cmd_agent(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    if sub == "prompt" {
        return cmd_prompt(&args[1..]);
    }
    let client = load_client()?;
    match sub {
        "list" | "ls" => {
            let company_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot agent list <companyId>"))?;
            let agents = client.list_agents(company_id)?;
            println!("{}", format_json(&agents));
            Ok(())
        }
        "get" => {
            let company_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot agent get <companyId> <agentId>"))?;
            let agent_id = args
                .get(2)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot agent get <companyId> <agentId>"))?;
            let agent = client.get_agent(company_id, agent_id)?;
            println!("{}", format_json(&agent));
            Ok(())
        }
        _ => {
            println!("Usage: parrot agent list <companyId> | get <companyId> <agentId> | prompt <text...>");
            Ok(())
        }
    }
}

// ── Issue ─────────────────────────────────────────────────────────────

fn cmd_issue(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let client = load_client()?;
    match sub {
        "list" | "ls" => {
            let company_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot issue list <companyId>"))?;
            let query = get_flag_value(args, "--q").or_else(|| get_flag_value(args, "--query"));
            let issues = client.list_issues(company_id, query.as_deref())?;
            println!("{}", format_json(&issues));
            Ok(())
        }
        "get" => {
            let company_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot issue get <companyId> <issueId>"))?;
            let issue_id = args
                .get(2)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot issue get <companyId> <issueId>"))?;
            let issue = client.get_issue(company_id, issue_id)?;
            println!("{}", format_json(&issue));
            Ok(())
        }
        _ => {
            println!("Usage: parrot issue list <companyId> | get <companyId> <issueId>");
            Ok(())
        }
    }
}

// ── Goal ─────────────────────────────────────────────────────────────

fn cmd_goal(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let client = load_client()?;
    match sub {
        "list" | "ls" => {
            let company_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot goal list <companyId>"))?;
            let goals = client.list_goals(company_id)?;
            println!("{}", format_json(&goals));
            Ok(())
        }
        _ => {
            println!("Usage: parrot goal list <companyId>");
            Ok(())
        }
    }
}

// ── Project ───────────────────────────────────────────────────────────

fn cmd_project(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let client = load_client()?;
    match sub {
        "list" | "ls" => {
            let company_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot project list <companyId>"))?;
            let projects = client.list_projects(company_id)?;
            println!("{}", format_json(&projects));
            Ok(())
        }
        _ => {
            println!("Usage: parrot project list <companyId>");
            Ok(())
        }
    }
}

// ── Secret ─────────────────────────────────────────────────────────────

fn cmd_secret(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let client = load_client()?;
    match sub {
        "list" | "ls" => {
            let company_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot secret list <companyId>"))?;
            let secrets = client.list_secrets(company_id)?;
            println!("{}", format_json(&secrets));
            Ok(())
        }
        _ => {
            println!("Usage: parrot secret list <companyId>");
            Ok(())
        }
    }
}

// ── Routine ────────────────────────────────────────────────────────────

fn cmd_routine(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let client = load_client()?;
    match sub {
        "list" | "ls" => {
            let company_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot routine list <companyId>"))?;
            let routines = client.list_routines(company_id)?;
            println!("{}", format_json(&routines));
            Ok(())
        }
        _ => {
            println!("Usage: parrot routine list <companyId>");
            Ok(())
        }
    }
}

// ── Activity ──────────────────────────────────────────────────────────

fn cmd_activity(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let client = load_client()?;
    match sub {
        "get" => {
            let company_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: parrot activity get <companyId>"))?;
            let activity = client.get_activity(company_id)?;
            println!("{}", format_json(&activity));
            Ok(())
        }
        _ => {
            println!("Usage: parrot activity get <companyId>");
            Ok(())
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

fn get_flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

fn get_flag_value_any(args: &[String], flags: &[&str]) -> Option<String> {
    flags.iter().find_map(|flag| get_flag_value(args, flag))
}


// ── Uninstall ─────────────────────────────────────────────────────────

fn cmd_uninstall(_args: &[String]) -> Result<()> {
    let paths = crate::install_store::InstallStorePaths::new();
    let self_path = std::env::current_exe()?;
    crate::install_store::remove_install_manifest(&paths)?;
    println!("removed install manifest");

    let home = std::env::var("HOME").unwrap_or_default();
    for candidate in [
        format!("{}/.local/bin/parrot", home),
        "/usr/local/bin/parrot".to_string(),
    ] {
        let p = std::path::PathBuf::from(&candidate);
        if p.exists() && p.canonicalize().ok() == self_path.canonicalize().ok() {
            std::fs::remove_file(&p)?;
            println!("removed {}", candidate);
        }
    }
    #[cfg(target_family = "unix")]
    {
        for unit in ["/etc/systemd/system/parrot.service",
                     &format!("{}/.config/systemd/user/parrot.service", home)] {
            if std::path::PathBuf::from(unit).exists() {
                std::fs::remove_file(unit)?;
                println!("removed {}", unit);
            }
        }
    }
    println!("uninstall complete");
    Ok(())
}

// ── Update ─────────────────────────────────────────────────────────────

fn cmd_update(args: &[String]) -> Result<()> {
    let version_flag = get_flag_value(args, "--version");
    let json = args.contains(&"--json".to_string());
    let paths = crate::install_store::InstallStorePaths::new();
    let current_manifest = crate::install_store::read_install_manifest(&paths)?;
    let current_version = current_manifest.as_ref().map(|m| m.current.version.clone()).unwrap_or_else(|| "unknown".to_string());

    // Check version compatibility with server
    if let Ok(config) = crate::config::CliConfig::load() {
        if let Ok(response) = reqwest::blocking::get(format!("{}/api/version", config.server_url)) {
            if let Ok(version_info) = response.json::<serde_json::Value>() {
                let server_version = version_info.get("version").and_then(|v| v.as_str()).unwrap_or("");
                let api_version = version_info.get("api_version").and_then(|v| v.as_str()).unwrap_or("");
                println!("server version: {}", server_version);
                println!("api version: {}", api_version);
                // Warn if API versions don't match (major version mismatch)
                if !api_version.is_empty() && !api_version.starts_with(&env!("CARGO_PKG_VERSION").split('.').take(1).collect::<String>()) {
                    println!("warning: API version mismatch may cause issues");
                }
            }
        }
    }

    let latest_version = if let Some(v) = version_flag {
        v
    } else {
        println!("update: checking for latest version...");
        println!("update: download from https://github.com/parrot/releases");
        return Ok(());
    };

    if latest_version == current_version {
        println!("already on latest version: {}", current_version);
        if json { println!("{}", serde_json::json!({ "status": "up-to-date", "version": current_version })); }
        return Ok(());
    }

    println!("updating from {} to {}", current_version, latest_version);
    if json {
        println!("{}", serde_json::json!({ "status": "update-available", "current_version": current_version, "latest_version": latest_version }));
    }
    Ok(())
}

// ── Onboard ────────────────────────────────────────────────────────────

fn cmd_onboard(args: &[String]) -> Result<()> {
    let server_url = get_flag_value(args, "--server-url");
    let api_token = get_flag_value(args, "--api-token");
    let config_path = get_flag_value(args, "--config").map(std::path::PathBuf::from);
    let run_server = args.contains(&"--run".to_string());
    let yes = args.contains(&"--yes".to_string());

    println!("Parrot Onboard");
    println!("==============");
    println!();

    let url = server_url.unwrap_or_else(|| {
        println!("Enter server URL (e.g., http://localhost:3100):");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        input.trim().to_string()
    });

    if !url.starts_with("http://") && !url.starts_with("https://") {
        bail!("server URL must start with http:// or https://");
    }

    let token = api_token.or_else(|| {
        if !yes {
            println!("Enter API token (optional, press Enter to skip):");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            let t = input.trim().to_string();
            if t.is_empty() { None } else { Some(t) }
        } else { None }
    });

    let path = resolve_config_path(config_path)
        .ok_or_else(|| anyhow::anyhow!("unable to determine config path; pass --config"))?;
    let existing_allowed_hostnames = crate::config::CliConfig::load_from(Some(path.clone()))
        .map(|config| config.allowed_hostnames)
        .unwrap_or_default();
    let config = crate::config::CliConfig {
        server_url: url,
        api_token: token,
        allowed_hostnames: existing_allowed_hostnames,
        config_path: Some(path.clone()),
    };
    config.save()?;
    println!("
configuration saved to {}", path.display());
    let telemetry_enabled = services::telemetry_service::TelemetryConfig::resolve().enabled;
    if !yes && telemetry_enabled {
        println!("
Telemetry is enabled. Set PARROT_TELEMETRY_DISABLED=1 to opt-out.");
    }

    if run_server {
        println!("
Starting Parrot server...");
        #[cfg(target_family = "unix")]
        {
            let status = std::process::Command::new("systemctl").args(["start", "parrot"]).status()?;
            if status.success() { println!("server started"); }
            else { println!("warning: failed to start server"); }
        }
    }

    println!("
onboard complete!");
    println!("
Next steps:");
    println!("  parrot doctor");
    println!("  parrot service status");
    Ok(())
}

// ── Fix ────────────────────────────────────────────────────────────────

fn cmd_fix(args: &[String]) -> Result<()> {
    let json = args.contains(&"--json".to_string());
    println!("Running Parrot fix diagnostics...
");

    let config = match crate::config::CliConfig::load() {
        Ok(c) => c,
        Err(e) => {
            println!("fix: no valid configuration ({})
Run: parrot configure --server-url <url>", e);
            if json { println!("{}", serde_json::json!({ "status": "error", "message": "no config" })); }
            return Ok(());
        }
    };

    let mut fixes_applied: Vec<String> = Vec::new();

    if let Some(path) = &config.config_path {
        if path.exists() { fixes_applied.push("config validated".to_string()); }
        else { println!("fix: config not found at {}", path.display()); }
    }

    match crate::client::ApiClient::new(config.server_url.clone(), config.api_token.clone()) {
        Ok(client) => match client.health_check() {
            Ok(_) => fixes_applied.push("server health check passed".to_string()),
            Err(e) => println!("fix: server unreachable ({})", e),
        },
        Err(e) => println!("fix: client init failed ({})", e),
    }

    let paths = crate::install_store::InstallStorePaths::new();
    match crate::install_store::read_install_manifest(&paths) {
        Ok(Some(m)) => fixes_applied.push(format!("manifest valid (v{})", m.current.version)),
        Ok(None) => println!("fix: no install manifest"),
        Err(e) => println!("fix: manifest error ({})", e),
    }

    println!("
fix complete: {} checks passed", fixes_applied.len());
    for fix in &fixes_applied { println!("  ✓ {}", fix); }

    if json { println!("{}", serde_json::json!({ "status": "complete", "fixes_applied": fixes_applied, "checks_passed": fixes_applied.len() })); }
    Ok(())
}
