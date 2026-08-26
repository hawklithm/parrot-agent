use anyhow::{bail, Result};
use std::path::PathBuf;

use crate::{
    backup, checks,
    config::{resolve_config_path, CliConfig},
};

pub fn run(args: impl IntoIterator<Item = String>) -> Result<()> {
    let args = args.into_iter().collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        None | Some("help") | Some("--help") | Some("-h") => print_help(),
        Some("--version") | Some("-V") | Some("version") => {
            println!("parrot {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("doctor") => {
            let (config_path, json) = parse_doctor_args(&args[1..])?;
            let config = match config_path {
                Some(path) => CliConfig::load_from(Some(path))?,
                None => CliConfig::load()?,
            };
            checks::run_doctor(&config, json)
        }
        Some("configure") => configure(&args[1..]),
        Some("db-backup") => backup::run(&args[1..]),
        Some(command) => bail!("unknown command '{command}'. Run 'parrot --help' for usage."),
    }
}

fn print_help() -> Result<()> {
    println!(
        "parrot {}\n\nUsage:\n  parrot --version\n  parrot doctor [--config PATH] [--json]\n  parrot configure --server-url URL [--api-token TOKEN] [--config PATH]\n  parrot db-backup [--connection-string URL] [--dir PATH] [--retention-days N] [--json]\n  parrot help\n\nEnvironment:\n  PARROT_SERVER_URL  Server base URL (default: http://localhost:3100)\n  PARROT_API_TOKEN   Optional API token; environment overrides config file\n  PARROT_CONFIG      Optional config file path; otherwise a platform default is used",
        env!("CARGO_PKG_VERSION")
    );
    Ok(())
}

fn parse_doctor_args(args: &[String]) -> Result<(Option<PathBuf>, bool)> {
    let mut config_path = None;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--config" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --config"))?;
                config_path = Some(PathBuf::from(value));
                index += 2;
            }
            "--json" => {
                json = true;
                index += 1;
            }
            flag => bail!("unknown doctor option '{flag}'"),
        }
    }
    Ok((config_path, json))
}

fn configure(args: &[String]) -> Result<()> {
    let mut server_url = None;
    let mut api_token = None;
    let mut config_path = None;
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args
            .get(index + 1)
            .ok_or_else(|| anyhow::anyhow!("missing value for {flag}"))?;
        match flag {
            "--server-url" => server_url = Some(value.clone()),
            "--api-token" => api_token = Some(value.clone()),
            "--config" => config_path = Some(PathBuf::from(value)),
            _ => bail!("unknown configure option '{flag}'"),
        }
        index += 2;
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
