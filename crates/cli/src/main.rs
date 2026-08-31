mod backup;
mod checks;
mod client;
mod commands;
mod plugin_scaffold;
mod config;
mod services;

use anyhow::Result;

fn main() -> Result<()> {
    commands::run(std::env::args().skip(1))
}
