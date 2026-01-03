mod browser;
mod database;
mod url_classify;
use std::time::{Duration, SystemTime};

pub use browser::{BrowserOpener, SystemBrowserOpener, open_address_impl};
pub use database::{Database, SqliteDatabase};
pub use url_classify::{InputType, classify_input};

use anyhow::{Context, Result};
use clap::Subcommand;
use log::info;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct OtotConfig {
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
            let config: OtotConfig = if let Some(path) = config_path {
                confy::load_path(path).context("Failed to load configuration")?
            } else {
                confy::load("otot", None).context("Failed to load configuration")?
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
            let mut config: OtotConfig = if let Some(path) = config_path {
                confy::load_path(path).unwrap_or_default()
            } else {
                confy::load("otot", None).context("Failed to load configuration")?
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
                        confy::store("otot", None, &config)
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
                confy::get_configuration_file_path("otot", None)
                    .context("Failed to get config path")?
                    .display()
                    .to_string()
            };

            println!("{}", config_path_display);
            Ok(())
        }
    }
}

pub fn format_relative_time(timestamp_secs: i64) -> String {
    let timestamp = std::time::UNIX_EPOCH + std::time::Duration::from_secs(timestamp_secs as u64);
    let elapsed = SystemTime::now()
        .duration_since(timestamp)
        .unwrap_or_default();

    let secs = elapsed.as_secs();

    match secs {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86399 => format!("{}h ago", secs / 3600),
        86400..=604799 => format!("{}d ago", secs / 86400),
        _ => format!("{}w ago", secs / 604800),
    }
}

pub fn parse_duration(s: &str) -> Result<Duration> {
    if s.is_empty() {
        anyhow::bail!("Duration cannot be empty");
    }

    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: u64 = num_str.parse().context("Invalid number in duration")?;

    let seconds = match unit {
        "d" => num * 86400,    // days
        "w" => num * 604800,   // weeks
        "m" => num * 2592000,  // months (30 days)
        "y" => num * 31536000, // years (365 days)
        _ => anyhow::bail!(
            "Invalid duration unit. Use d (days), w (weeks), m (months), or y (years)"
        ),
    };

    Ok(Duration::from_secs(seconds))
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

        let config: OtotConfig = confy::load_path(&config_path).unwrap();
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

        let config: OtotConfig = confy::load_path(&config_path).unwrap();
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
