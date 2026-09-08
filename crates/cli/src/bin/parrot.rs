use anyhow::Result;
use parrot_cli_lib::commands;

fn main() -> Result<()> {
    commands::run(std::env::args().skip(1))
}
