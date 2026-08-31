use anyhow::{bail, Result};
use std::path::PathBuf;

use crate::{
    backup, checks,
    client::ApiClient,
    config::resolve_config_path,
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
        "configure" => cmd_configure(rest),
        "db-backup" => backup::run(rest),
        "auth" => cmd_auth(rest),
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
        "service" => cmd_service(rest),
        "install" => cmd_install(rest),
        "update" => cmd_update(rest),
        _ => bail!("unknown command '{command}'. Run 'parrot help' for usage."),
    }
}

fn get_version() -> Result<()> {
    println!("parrot {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}

fn print_help() -> Result<()> {
    println!("parrot {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Usage: parrot <command> [options]");
    println!();
    println!("Configuration:");
    println!("  configure   --server-url URL [--api-token TOKEN]");
    println!("  doctor      [--json]");
    println!();
    println!("Data commands:");
    println!("  auth        get-session");
    println!("  company     list | get <id> | create | delete <id> | export <id> | import");
    println!("  agent       list <companyId> | get <companyId> <agentId>");
    println!("  issue       list <companyId> | get <companyId> <issueId>");
    println!("  goal        list <companyId>");
    println!("  project     list <companyId>");
    println!("  secret      list <companyId>");
    println!("  routine     list <companyId>");
    println!("  activity    get <companyId>");
    println!("  approval    list <companyId> | get <id> | approve <id> | reject <id> | resubmit <id>");
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
    println!();
    println!("Server management:");
    println!("  service     status | start | stop | restart");
    println!("  install     [--dir PATH]");
    println!("  update      [--version VERSION]");
    println!();
    println!("Maintenance:");
    println!("  db-backup   [--dir PATH] [--retention-days N]");
    println!("  version");
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
        let val = args.get(i + 1).ok_or_else(|| anyhow::anyhow!("missing value for {flag}"))?;
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
    let config = crate::config::CliConfig {
        server_url: url,
        api_token,
        config_path: Some(path.clone()),
    };
    config.save()?;
    println!("configuration saved to {}", path.display());
    Ok(())
}

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
        _ => {
            println!("Usage: parrot service status | start | stop | restart");
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
            { PathBuf::from("/usr/local/bin") }
            #[cfg(target_family = "windows")]
            { PathBuf::from(std::env::var("PROGRAMFILES").unwrap_or_else(|_| "C:\\Program Files\\parrot".into())) }
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
    }
    #[cfg(target_family = "windows")]
    {
        std::fs::create_dir_all(&install_dir)?;
        std::fs::copy(&self_path, &target_path)?;
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
        _ => {
            println!("Usage: parrot auth get-session");
            Ok(())
        }
    }
}


// ── Update ─────────────────────────────────────────────────────────────

fn cmd_update(args: &[String]) -> Result<()> {
    let _version = get_flag_value(args, "--version");
    println!("update: this command will download the latest parrot release");
    println!("update: not yet implemented — download from https://github.com/parrot/releases");
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
            let id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot company get <id>"))?;
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
            let id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot company delete <id>"))?;
            println!("{}", format_json(&client.delete_company(id)?));
            Ok(())
        }
        "export" => {
            let id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot company export <id>"))?;
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
            let company_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot approval list <companyId>"))?;
            println!("{}", format_json(&client.list_approvals(company_id)?));
            Ok(())
        }
        "get" => {
            let id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot approval get <id>"))?;
            println!("{}", format_json(&client.get_approval(id)?));
            Ok(())
        }
        "approve" => {
            let id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot approval approve <id> [--json '{{...}}']"))?;
            let body = approval_body(args);
            println!("{}", format_json(&client.approve_approval(id, &body)?));
            Ok(())
        }
        "reject" => {
            let id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot approval reject <id> [--json '{{...}}']"))?;
            let body = approval_body(args);
            println!("{}", format_json(&client.reject_approval(id, &body)?));
            Ok(())
        }
        "resubmit" => {
            let id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot approval resubmit <id> [--json '{{...}}']"))?;
            let body = get_flag_value(args, "--json").map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::Null));
            println!("{}", format_json(&client.resubmit_approval(id, body.as_ref())?));
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
            let company_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot pipeline list <companyId>"))?;
            println!("{}", format_json(&client.list_pipelines(company_id)?));
            Ok(())
        }
        "get" => {
            let id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot pipeline get <id>"))?;
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
            let name = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot skill get <name>"))?;
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
    let client = load_client()?;
    match sub {
        "list" | "ls" => {
            println!("{}", format_json(&client.list_plugins()?));
            Ok(())
        }
        "install" => {
            let raw = get_flag_value(args, "--json")
                .ok_or_else(|| anyhow::anyhow!("plugin install requires --json '{{...}}'"))?;
            let body: serde_json::Value = serde_json::from_str(&raw)?;
            println!("{}", format_json(&client.install_plugin(&body)?));
            Ok(())
        }
        "enable" => {
            let id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot plugin enable <id>"))?;
            println!("{}", format_json(&client.enable_plugin(id)?));
            Ok(())
        }
        "create" => {
            // Offline scaffolding: `parrot plugin create <name> --output <dir>
            // [--template default|connector|workspace|environment]`
            // (Paperclip create-paperclip-plugin equivalent).
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
        "disable" => {
            let id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot plugin disable <id>"))?;
            println!("{}", format_json(&client.disable_plugin(id)?));
            Ok(())
        }
        _ => {
            println!("Usage: parrot plugin list | create <name> --output <dir> | install --json '{{...}}' | enable <id> | disable <id>");
            Ok(())
        }
    }
}

// ── Dashboard ─────────────────────────────────────────────────────

fn cmd_dashboard(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("get");
    let client = load_client()?;
    match sub {
        "get" => {
            let company_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot dashboard get <companyId>"))?;
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
            let company_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot cost summary <companyId>"))?;
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
            let trace_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot feedback get <traceId>"))?;
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
            let company_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot access org-chart <companyId>"))?;
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
            let company_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot workspace list <companyId>"))?;
            println!("{}", format_json(&client.list_workspaces(company_id)?));
            Ok(())
        }
        "get" => {
            let id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot workspace get <id>"))?;
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
            let issue_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot run list <issueId>"))?;
            println!("{}", format_json(&client.list_issue_runs(issue_id)?));
            Ok(())
        }
        "get" => {
            let id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot run get <id>"))?;
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
            let company_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot channel list <companyId>"))?;
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
    let client = load_client()?;
    match sub {
        "list" | "ls" => {
            let company_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot agent list <companyId>"))?;
            let agents = client.list_agents(company_id)?;
            println!("{}", format_json(&agents));
            Ok(())
        }
        "get" => {
            let company_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot agent get <companyId> <agentId>"))?;
            let agent_id = args.get(2).ok_or_else(|| anyhow::anyhow!("Usage: parrot agent get <companyId> <agentId>"))?;
            let agent = client.get_agent(company_id, agent_id)?;
            println!("{}", format_json(&agent));
            Ok(())
        }
        _ => {
            println!("Usage: parrot agent list <companyId> | get <companyId> <agentId>");
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
            let company_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot issue list <companyId>"))?;
            let query = get_flag_value(args, "--q").or_else(|| get_flag_value(args, "--query"));
            let issues = client.list_issues(company_id, query.as_deref())?;
            println!("{}", format_json(&issues));
            Ok(())
        }
        "get" => {
            let company_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot issue get <companyId> <issueId>"))?;
            let issue_id = args.get(2).ok_or_else(|| anyhow::anyhow!("Usage: parrot issue get <companyId> <issueId>"))?;
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
            let company_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot goal list <companyId>"))?;
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
            let company_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot project list <companyId>"))?;
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
            let company_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot secret list <companyId>"))?;
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
            let company_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot routine list <companyId>"))?;
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
            let company_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot activity get <companyId>"))?;
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
