use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use confy;
use serde::{Deserialize, Serialize};
use zurl::{ConfigAction, handle_config, handle_open};

#[derive(Debug, Default, Serialize, Deserialize)]
struct ZurlConfig {
    preferred_browser: Option<String>,
}

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    #[command(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,
}

#[derive(Subcommand)]
enum Command {
    Open {
        address: String,
    },
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

fn main() -> Result<()> {
    let args = Cli::parse();

    env_logger::Builder::new()
        .filter_level(args.verbosity.into())
        .init();

    let cfg: ZurlConfig = confy::load("zurl", None).context("Failed to load configuration")?;

    match args.command {
        Command::Open { address } => handle_open(&address, cfg.preferred_browser.as_deref())?,
        Command::Config { action } => handle_config(action)?,
    }
    Ok(())
}
