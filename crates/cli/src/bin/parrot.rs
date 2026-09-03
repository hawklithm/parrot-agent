mod backup;
mod checks;
mod client;
mod commands;
mod config;
mod install_store;
mod plugin_scaffold;
mod services;

mod update_notice;
use anyhow::Result;

fn main() -> Result<()> {
    commands::run(std::env::args().skip(1))
}
