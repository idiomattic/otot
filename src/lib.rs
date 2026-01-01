mod browser;
mod database;
mod url_classify;

pub use browser::{BrowserOpener, SystemBrowserOpener, open_address_impl};
pub use database::{Database, SqliteDatabase};
pub use url_classify::{InputType, classify_input};

use anyhow::{Context, Result};
use clap::Subcommand;
use log::info;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ZurlConfig {
    pub preferred_browser: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    Set {
        #[arg(short, long)]
        key: String,

        #[arg(short, long)]
        new: String,
    },
    Get {
        #[arg(short, long)]
        key: String,
    },
    Path,
}

pub fn handle_config_action(action: ConfigAction) -> Result<()> {
    handle_config_action_with_config(action, None)
}
pub fn handle_config_action_with_config(
    action: ConfigAction,
    config_path: Option<&std::path::Path>,
) -> Result<()> {
    match action {
        ConfigAction::Get { key } => {
            let config: ZurlConfig = if let Some(path) = config_path {
                confy::load_path(path).context("Failed to load configuration")?
            } else {
                confy::load("zurl", None).context("Failed to load configuration")?
            };

            match key.as_str() {
                "preferred_browser" => match &config.preferred_browser {
                    Some(browser) => println!("{}", browser),
                    None => println!("(not set)"),
                },
                _ => {
                    anyhow::bail!(
                        "Unknown config key: '{}'. Valid keys: preferred_browser",
                        key
                    );
                }
            }

            Ok(())
        }

        ConfigAction::Set { key, new } => {
            let mut config: ZurlConfig = if let Some(path) = config_path {
                confy::load_path(path).unwrap_or_default()
            } else {
                confy::load("zurl", None).context("Failed to load configuration")?
            };

            match key.as_str() {
                "preferred_browser" => {
                    let new_value = if new.is_empty() {
                        None
                    } else {
                        Some(new.clone())
                    };

                    config.preferred_browser = new_value;

                    if let Some(path) = config_path {
                        confy::store_path(path, &config).context("Failed to save configuration")?;
                    } else {
                        confy::store("zurl", None, &config)
                            .context("Failed to save configuration")?;
                    }

                    info!(
                        "Set preferred_browser to: {}",
                        config.preferred_browser.as_deref().unwrap_or("(none)")
                    );
                    println!("Configuration updated");
                }
                _ => {
                    anyhow::bail!(
                        "Unknown config key: '{}'. Valid keys: preferred_browser",
                        key
                    );
                }
            }

            Ok(())
        }

        ConfigAction::Path => {
            let config_path_display = if let Some(path) = config_path {
                path.display().to_string()
            } else {
                confy::get_configuration_file_path("zurl", None)
                    .context("Failed to get config path")?
                    .display()
                    .to_string()
            };

            println!("{}", config_path_display);
            Ok(())
        }
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;
    use assert_fs::TempDir;
    #[test]
    fn config_set_and_get_preferred_browser() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        handle_config_action_with_config(
            ConfigAction::Set {
                key: "preferred_browser".to_string(),
                new: "firefox".to_string(),
            },
            Some(&config_path),
        )
        .unwrap();

        let config: ZurlConfig = confy::load_path(&config_path).unwrap();
        assert_eq!(config.preferred_browser, Some("firefox".to_string()));

        let result = handle_config_action_with_config(
            ConfigAction::Get {
                key: "preferred_browser".to_string(),
            },
            Some(&config_path),
        );
        assert!(result.is_ok());
    }
    #[test]
    fn config_set_empty_value_clears_setting() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        handle_config_action_with_config(
            ConfigAction::Set {
                key: "preferred_browser".to_string(),
                new: "firefox".to_string(),
            },
            Some(&config_path),
        )
        .unwrap();

        handle_config_action_with_config(
            ConfigAction::Set {
                key: "preferred_browser".to_string(),
                new: "".to_string(),
            },
            Some(&config_path),
        )
        .unwrap();

        let config: ZurlConfig = confy::load_path(&config_path).unwrap();
        assert_eq!(config.preferred_browser, None);
    }
    #[test]
    fn config_get_unknown_key() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let result = handle_config_action_with_config(
            ConfigAction::Get {
                key: "nonexistent_key".to_string(),
            },
            Some(&config_path),
        );

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unknown config key")
        );
    }
    #[test]
    fn config_set_unknown_key() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let result = handle_config_action_with_config(
            ConfigAction::Set {
                key: "nonexistent_key".to_string(),
                new: "some_value".to_string(),
            },
            Some(&config_path),
        );

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unknown config key")
        );
    }
    #[test]
    fn config_path_shows_custom_path() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let result = handle_config_action_with_config(ConfigAction::Path, Some(&config_path));

        assert!(result.is_ok());
    }
    #[test]
    fn config_get_when_file_does_not_exist_shows_not_set() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("nonexistent.toml");

        let result = handle_config_action_with_config(
            ConfigAction::Get {
                key: "preferred_browser".to_string(),
            },
            Some(&config_path),
        );

        assert!(result.is_ok());
    }
}
