use anyhow::{Context, Result};
use clap::Parser;
use confy;
use log::debug;
use open;
use serde::{Deserialize, Serialize};
use zurl::{InputType, classify_input};

#[derive(Debug, Default, Serialize, Deserialize)]
struct ZurlConfig {
    preferred_browser: Option<String>,
}

#[derive(Parser)]
struct Cli {
    address: String,
    #[command(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,
}

fn main() -> Result<()> {
    let args = Cli::parse();

    env_logger::Builder::new()
        .filter_level(args.verbosity.into())
        .init();

    let cfg: ZurlConfig = confy::load("zurl", None).context("Failed to load configuration")?;

    if args.address.is_empty() {
        anyhow::bail!("provided address must be a non-empty string");
    }

    let parsed = classify_input(&args.address);
    match parsed {
        InputType::FullUrl(url) => {
            debug!("Parsed FullUrl {:?}", &url);
            match cfg.preferred_browser {
                Some(browser) => {
                    debug!("Opening link with {:?}", &browser);
                    open::with(url.as_str(), browser)?
                }
                None => open::that(url.as_str())?,
            }
        }
        InputType::FuzzyPattern(_segments) => {
            anyhow::bail!("Opening links from a fuzzy pattern is not implemented yet!")
        }
    }
    Ok(())
}
