use anyhow::{bail, Result};

use crate::{checks, config::CliConfig};

pub fn run(args: impl IntoIterator<Item = String>) -> Result<()> {
    let args = args.into_iter().collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        None | Some("help") | Some("--help") | Some("-h") => print_help(),
        Some("--version") | Some("-V") | Some("version") => {
            println!("parrot {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("doctor") => {
            let config = CliConfig::load()?;
            checks::run_doctor(&config)
        }
        Some(command) => bail!("unknown command '{command}'. Run 'parrot --help' for usage."),
    }
}

fn print_help() -> Result<()> {
    println!(
        "parrot {}\n\nUsage:\n  parrot --version\n  parrot doctor\n  parrot help\n\nEnvironment:\n  PARROT_SERVER_URL  Server base URL (default: http://localhost:3100)\n  PARROT_API_TOKEN   Optional API token used by future API commands\n  PARROT_CONFIG      Optional config file path",
        env!("CARGO_PKG_VERSION")
    );
    Ok(())
}

