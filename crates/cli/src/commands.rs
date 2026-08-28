use anyhow::{bail, Result};
use std::path::PathBuf;

use crate::{
    backup, checks,
    client::ApiClient,
    config::{resolve_config_path, CliConfig},
};

pub fn run(args: impl IntoIterator<Item = String>) -> Result<()> {
    let raw: Vec<String> = args.into_iter().collect();
    let (command, rest) = match raw.first() {
        None => return print_help(),
        Some(c) => (c.as_str(), &raw[1..]),
    };
    match command {
        "help" | "--help" | "-h" => print_help(),
        "version" | "--version" | "-V" => {
            println!("parrot {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
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
        _ => bail!("unknown command '{command}'. Run 'parrot help' for usage."),
    }
}

fn print_help() -> Result<()> {
    println!("parrot {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Usage: parrot <command> [options]");
    println!();
    println!("Configuration:");
    println!("  configure   --server-url URL [--api-token TOKEN] [--config PATH]");
    println!("  doctor      [--config PATH] [--json]");
    println!();
    println!("Data commands:");
    println!("  auth        get-session");
    println!("  company     list | get <id>");
    println!("  agent       list <companyId> | get <companyId> <agentId>");
    println!("  issue       list <companyId> [--q QUERY] | get <companyId> <issueId>");
    println!("  goal        list <companyId>");
    println!("  project     list <companyId>");
    println!("  secret      list <companyId>");
    println!("  routine     list <companyId>");
    println!("  activity    get <companyId>");
    println!();
    println!("Maintenance:");
    println!("  db-backup   [--connection-string URL] [--dir PATH] [--retention-days N]");
    println!("  version     Show version");
    println!("  help        Show this help");
    Ok(())
}

// ── Shared helpers ────────────────────────────────────────────────────

fn load_client() -> Result<ApiClient> {
    let config = CliConfig::load()?;
    ApiClient::new(config.server_url, config.api_token)
}

fn format_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| format!("{value}"))
}

// ── Doctor ──────────────────────────────────────────────────────────

fn cmd_doctor(args: &[String]) -> Result<()> {
    let (config_path, json) = parse_flag_args(args, &["--json"], &["--config"])?;
    let config = match config_path {
        Some(path) => CliConfig::load_from(Some(path))?,
        None => CliConfig::load()?,
    };
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
    let config = CliConfig {
        server_url: url,
        api_token,
        config_path: Some(path.clone()),
    };
    config.save()?;
    println!("configuration saved to {}", path.display());
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

// ── Company ───────────────────────────────────────────────────────────

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
        _ => {
            println!("Usage: parrot company list | get <id>");
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
            let company_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Usage: parrot issue list <companyId> [--q QUERY]"))?;
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
            println!("Usage: parrot issue list <companyId> [--q QUERY] | get <companyId> <issueId>");
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

// ── Flag helpers ──────────────────────────────────────────────────────

fn parse_flag_args(
    args: &[String],
    bool_flags: &[&str],
    value_flags: &[&str],
) -> Result<(Option<PathBuf>, bool)> {
    let mut config_path = None;
    let mut json = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                let val = args.get(i + 1).ok_or_else(|| anyhow::anyhow!("missing value for --config"))?;
                config_path = Some(PathBuf::from(val));
                i += 2;
            }
            "--json" => { json = true; i += 1; }
            flag => bail!("unknown option '{flag}'"),
        }
    }
    Ok((config_path, json))
}

fn get_flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}
