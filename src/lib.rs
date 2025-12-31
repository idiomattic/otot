mod browser;
mod database;
mod url_classify;

pub use browser::{BrowserOpener, SystemBrowserOpener, open_address_impl};
pub use database::{Database, SqliteDatabase};
pub use url_classify::{InputType, classify_input};

use anyhow::Result;
use clap::Subcommand;
use log::debug;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ZurlConfig {
    pub preferred_browser: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    Set { key: String, value: String },
    Get { key: String },
    Path,
}

pub fn handle_config_action(action: ConfigAction) -> Result<()> {
    debug!("Received config action: {:?}", &action);
    anyhow::bail!("Config command is not implemented yet!")
}
