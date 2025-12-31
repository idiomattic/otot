use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use confy;
use serde::{Deserialize, Serialize};
use zurl::{ConfigAction, handle_config_action, handle_open_address};

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

struct App {
    config: ZurlConfig,
    // db connection, etc.
    // logger?
}

impl App {
    fn new() -> Result<Self> {
        let config = confy::load("zurl", None).context("Failed to load configuration")?;
        Ok(Self { config })
    }

    fn handle_open(&self, address: &str) -> Result<()> {
        handle_open_address(address, self.config.preferred_browser.as_deref())
    }

    fn handle_config(&self, action: ConfigAction) -> Result<()> {
        handle_config_action(action)
    }
}

fn main() -> Result<()> {
    let args = Cli::parse();

    env_logger::Builder::new()
        .filter_level(args.verbosity.into())
        .init();

    let app = App::new()?;

    match args.command {
        Command::Open { address } => app.handle_open(&address)?,
        Command::Config { action } => app.handle_config(action)?,
    }

    Ok(())
}
