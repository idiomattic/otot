use anyhow::Result;
use clap::{Parser, Subcommand};
use confy;
use serde::{Deserialize, Serialize};
use zurl::{ConfigAction, handle_config_action, open_address_impl, open_url};

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

#[derive(Default)]
struct AppBuilder {
    config: Option<ZurlConfig>,
    opener: Option<Box<dyn Fn(&str, Option<&str>) -> std::io::Result<()>>>,
}

impl AppBuilder {
    #[cfg(test)]
    fn with_config(mut self, config: ZurlConfig) -> Self {
        self.config = Some(config);
        self
    }

    #[cfg(test)]
    fn with_opener<F>(mut self, opener: F) -> Self
    where
        F: Fn(&str, Option<&str>) -> std::io::Result<()> + 'static,
    {
        self.opener = Some(Box::new(opener));
        self
    }

    fn build(self) -> Result<App> {
        let config = self.config.unwrap_or_else(|| {
            confy::load("zurl", None).expect("Failed to load config in builder")
        });

        let opener = self.opener.unwrap_or_else(|| Box::new(open_url));

        Ok(App { config, opener })
    }
}

struct App {
    config: ZurlConfig,
    // Box gives us a fixed-size pointer to the dynamic function
    // compiler needs to know the size of this struct, so we can't use the dynamic function without wrapping
    opener: Box<dyn Fn(&str, Option<&str>) -> std::io::Result<()>>,
    // db connection, etc.
}

impl App {
    fn builder() -> AppBuilder {
        AppBuilder::default()
    }

    fn new() -> Result<Self> {
        Self::builder().build()
    }

    fn handle_open(&self, address: &str) -> Result<()> {
        open_address_impl(
            &*self.opener,
            address,
            self.config.preferred_browser.as_deref(),
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    #[test]
    fn app_opens_url_with_mock_opener() {
        let captured = Rc::new(RefCell::new(None));
        let captured_clone = captured.clone();
        let mock_opener = move |url: &str, browser: Option<&str>| {
            *captured_clone.borrow_mut() = Some((url.to_string(), browser.map(String::from)));
            Ok(())
        };
        let app = AppBuilder::default()
            .with_opener(mock_opener)
            .build()
            .unwrap();
        app.handle_open("github.com").unwrap();
        assert_eq!(
            *captured.borrow(),
            Some(("https://github.com/".to_string(), None))
        );
    }
    #[test]
    fn app_uses_preferred_browser_from_config() {
        let captured = Rc::new(RefCell::new(None));
        let captured_clone = captured.clone();
        let mock_opener = move |url: &str, browser: Option<&str>| {
            *captured_clone.borrow_mut() = Some((url.to_string(), browser.map(String::from)));
            Ok(())
        };
        let config = ZurlConfig {
            preferred_browser: Some("firefox".to_string()),
        };

        let app = AppBuilder::default()
            .with_config(config)
            .with_opener(mock_opener)
            .build()
            .unwrap();
        app.handle_open("github.com").unwrap();
        assert_eq!(
            *captured.borrow(),
            Some((
                "https://github.com/".to_string(),
                Some("firefox".to_string())
            ))
        );
    }
    #[test]
    fn app_builder_uses_defaults_when_not_specified() {
        let result = AppBuilder::default().build();
        assert!(result.is_ok());
    }
}
