use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use confy;
use zurl::{
    BrowserOpener, ConfigAction, Database, SqliteDatabase, SystemBrowserOpener, ZurlConfig,
    handle_config_action, open_address_impl,
};

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
    opener: Option<Box<dyn BrowserOpener>>,
    db: Option<Box<dyn Database>>,
}

impl AppBuilder {
    #[cfg(test)]
    fn with_config(mut self, config: ZurlConfig) -> Self {
        self.config = Some(config);
        self
    }

    #[cfg(test)]
    fn with_opener<O>(mut self, opener: O) -> Self
    where
        O: BrowserOpener + 'static,
    {
        self.opener = Some(Box::new(opener));
        self
    }

    fn build(self) -> Result<App> {
        let config = match self.config {
            Some(c) => c,
            None => confy::load("zurl", None).context("Failed to load config in builder")?,
        };

        let opener = self.opener.unwrap_or_else(|| Box::new(SystemBrowserOpener));

        let db = self
            .db
            .unwrap_or_else(|| Box::new(SqliteDatabase::open().expect("Failed to open database")));

        Ok(App { config, opener, db })
    }
}

struct App {
    config: ZurlConfig,
    // Box gives us a fixed-size pointer to the dynamic trait - compiler needs to know size
    opener: Box<dyn BrowserOpener>,
    db: Box<dyn Database>,
}

impl App {
    fn builder() -> AppBuilder {
        AppBuilder::default()
    }

    fn new() -> Result<Self> {
        Self::builder().build()
    }

    fn handle_open(&mut self, address: &str) -> Result<()> {
        open_address_impl(
            self.opener.as_ref(),
            self.db.as_mut(),
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

    let mut app = App::new()?;

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

    pub struct MockBrowserOpener {
        pub captured: std::rc::Rc<std::cell::RefCell<Option<(String, Option<String>)>>>,
    }

    impl BrowserOpener for MockBrowserOpener {
        fn open(&self, url: &str, browser: Option<&str>) -> std::io::Result<()> {
            *self.captured.borrow_mut() = Some((url.to_string(), browser.map(String::from)));
            Ok(())
        }
    }

    #[test]
    fn app_opens_url_with_mock_opener() {
        let captured = Rc::new(RefCell::new(None));
        let mock = MockBrowserOpener {
            captured: captured.clone(),
        };

        let mut app = AppBuilder::default().with_opener(mock).build().unwrap();
        app.handle_open("github.com").unwrap();
        assert_eq!(
            *captured.borrow(),
            Some(("https://github.com/".to_string(), None))
        );
    }
    #[test]
    fn app_uses_preferred_browser_from_config() {
        let captured = Rc::new(RefCell::new(None));
        let mock = MockBrowserOpener {
            captured: captured.clone(),
        };

        let config = ZurlConfig {
            preferred_browser: Some("firefox".to_string()),
        };

        let mut app = AppBuilder::default()
            .with_config(config)
            .with_opener(mock)
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
