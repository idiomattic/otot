mod browser;
mod database;
mod url_classify;

pub use browser::{BrowserOpener, SystemBrowserOpener, open_address_impl};
pub use database::{Database, SqliteDatabase};
pub use url_classify::{InputType, classify_input};

use anyhow::{Context, Result};
use clap::Subcommand;
use log::{debug, info};
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
    match action {
        ConfigAction::Get { key } => {
            let config: ZurlConfig =
                confy::load("zurl", None).context("Failed to load configuration")?;

            match key.as_str() {
                "preferred_browser" => match &config.preferred_browser {
                    Some(browser) => println!("{}", browser),
                    None => println!("(not set)"),
                },
                _ => anyhow::bail!("Unknown config key: '{}'", key),
            }

            Ok(())
        }
        ConfigAction::Set { key, value } => {
            let mut config: ZurlConfig =
                confy::load("zurl", None).context("Failed to load configuration")?;

            match key.as_str() {
                "preferred_browser" => {
                    let new_value = if value.is_empty() {
                        None
                    } else {
                        Some(value.clone())
                    };

                    config.preferred_browser = new_value;
                    confy::store("zurl", None, &config).context("Failed to save configuration")?;

                    info!(
                        "Set preferred browser to: {}",
                        config.preferred_browser.as_deref().unwrap_or("(none}")
                    );
                    println!("Configuration updated");
                }
                _ => anyhow::bail!("Unknown config key: '{}'", key),
            }

            Ok(())
        }
        ConfigAction::Path => {
            let config_path = confy::get_configuration_file_path("zurl", None)
                .context("Failed to get config path")?;

            println!("{}", config_path.display());
            Ok(())
        }
    }
}
