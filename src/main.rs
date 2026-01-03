use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use otot::{
    BrowserOpener, ConfigAction, Database, InputType, OtotConfig, SqliteDatabase,
    SystemBrowserOpener, classify_input, format_relative_time, handle_config_action,
    open_address_impl,
};

#[derive(Parser)]
#[command(version)]
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
    Query {
        address: String,
    },
    Stats {
        #[arg(short, long, default_value = "10")]
        size: u16,
    },
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Default)]
struct AppBuilder {
    config: Option<OtotConfig>,
    opener: Option<Box<dyn BrowserOpener>>,
    db: Option<Box<dyn Database>>,
}

impl AppBuilder {
    #[cfg(test)]
    fn with_config(mut self, config: OtotConfig) -> Self {
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

    #[cfg(test)]
    fn with_db<D>(mut self, db: D) -> Self
    where
        D: Database + 'static,
    {
        self.db = Some(Box::new(db));
        self
    }

    fn build(self) -> Result<App> {
        let config = match self.config {
            Some(c) => c,
            None => confy::load("otot", None).context("Failed to load config in builder")?,
        };

        // Options because these components aren't required for all subcommands (e.g. `otot config` does not require either)
        //  and we can skip the extra overhead from their initialization.
        let opener = self.opener;
        let db = self.db;

        Ok(App { config, opener, db })
    }
}

struct App {
    config: OtotConfig,
    // Box gives us a fixed-size pointer to the dynamic trait - compiler needs to know size
    // These are Option so we can avoid initializing them for config commands
    opener: Option<Box<dyn BrowserOpener>>,
    db: Option<Box<dyn Database>>,
}

impl App {
    fn builder() -> AppBuilder {
        AppBuilder::default()
    }

    fn new() -> Result<Self> {
        Self::builder().build()
    }

    fn handle_open(&mut self, address: &str) -> Result<()> {
        // Lazy initialization: only create opener and db when actually opening a URL
        let opener = self
            .opener
            .get_or_insert_with(|| Box::new(SystemBrowserOpener));
        let db = self.db.get_or_insert_with(|| {
            Box::new(SqliteDatabase::open().expect("Failed to open database"))
        });

        open_address_impl(
            opener.as_ref(),
            db.as_mut(),
            address,
            self.config.preferred_browser.as_deref(),
        )
    }

    fn handle_query(&mut self, address: &str) -> Result<()> {
        let db = self.db.get_or_insert_with(|| {
            Box::new(SqliteDatabase::open().expect("Failed to open database"))
        });

        match classify_input(address) {
            InputType::FullUrl(_url) => {
                anyhow::bail!("Queried a fully-qualified URL which would be opened directly.")
            }
            InputType::FuzzyPattern(segments) => {
                let matches = db.fuzzy_match(&segments)?;
                if !matches.is_empty() {
                    println!("{:<50} {:>8} {:>15}", "URL", "SCORE", "LAST VISITED");
                    println!("{}", "-".repeat(75));
                    for (match_url, score, last_accessed) in matches {
                        let () = println!(
                            "{:<50} {:>8.1} {:>12}",
                            match_url,
                            score,
                            format_relative_time(last_accessed)
                        );
                    }
                    Ok(())
                } else {
                    anyhow::bail!("No matches found for pattern");
                }
            }
        }
    }

    fn handle_stats(&mut self, size: u16) -> Result<()> {
        let db = self.db.get_or_insert_with(|| {
            Box::new(SqliteDatabase::open().expect("Failed to open database"))
        });

        let top_urls = db.get_highest_usage_urls(size)?;

        if top_urls.is_empty() {
            println!("No URLs in history yet.");
            return Ok(());
        }

        println!("Top {} Most Visited URLs\n", size);
        println!("{:<50} {:>8} {:>15}", "URL", "SCORE", "LAST VISITED");
        println!("{}", "-".repeat(75));

        for (url, score, last_accessed) in top_urls {
            println!(
                "{:<50} {:>8.1} {:>12}",
                url,
                score,
                format_relative_time(last_accessed)
            );
        }

        Ok(())
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
        Command::Query { address } => app.handle_query(&address)?,
        Command::Stats { size } => app.handle_stats(size)?,
        Command::Config { action } => app.handle_config(action)?,
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    type CapturedCall = (String, Option<String>);
    type SharedCapture = Rc<RefCell<Option<CapturedCall>>>;
    pub struct MockBrowserOpener {
        pub captured: SharedCapture,
    }

    impl BrowserOpener for MockBrowserOpener {
        fn open(&self, url: &str, browser: Option<&str>) -> std::io::Result<()> {
            *self.captured.borrow_mut() = Some((url.to_string(), browser.map(String::from)));
            Ok(())
        }
    }

    struct MockDatabase;

    impl Database for MockDatabase {
        fn add_visit(
            &mut self,
            _url: &str,
            _timestamp: std::time::SystemTime,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn fuzzy_match(&self, _pattern: &[String]) -> anyhow::Result<Vec<(String, f64, i64)>> {
            Ok(vec![])
        }

        fn get_best_match(&self, _pattern: &[String]) -> anyhow::Result<Option<String>> {
            Ok(None)
        }

        fn get_highest_usage_urls(&self, _size: u16) -> Result<Vec<(String, f64, i64)>> {
            Ok(vec![])
        }
    }

    #[test]
    fn app_opens_url_with_mock_opener() {
        let captured = Rc::new(RefCell::new(None));
        let mock = MockBrowserOpener {
            captured: captured.clone(),
        };
        let mut app = AppBuilder::default()
            .with_config(OtotConfig::default())
            .with_opener(mock)
            .with_db(MockDatabase)
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
        let mock = MockBrowserOpener {
            captured: captured.clone(),
        };

        let config = OtotConfig {
            preferred_browser: Some("firefox".to_string()),
        };

        let mut app = AppBuilder::default()
            .with_config(config)
            .with_opener(mock)
            .with_db(MockDatabase)
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

    #[test]
    fn config_commands_dont_initialize_db_or_opener() {
        // Build an app without providing db or opener
        let app = AppBuilder::default()
            .with_config(OtotConfig::default())
            .build()
            .unwrap();

        // Verify that db and opener are None (not initialized)
        assert!(app.db.is_none());
        assert!(app.opener.is_none());

        // Config commands should work fine without them
        let result = app.handle_config(ConfigAction::Path);
        assert!(result.is_ok());
    }
}
