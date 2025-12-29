use anyhow::Result;
use clap::Parser;
use confy;
use log::info;
use open;
use serde::{Deserialize, Serialize};
use zurl::{InputType, classify_input};

#[derive(Debug, Default, Serialize, Deserialize)]
struct ZurlConfig {
    preferred_browser: String,
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

    let cfg: ZurlConfig = confy::load("zurl", None)?;

    if args.address.is_empty() {
        anyhow::bail!("provided address must be a non-empty string");
    }

    let parsed = classify_input(&args.address);
    match parsed {
        InputType::FullUrl(url) => {
            info!("Parsed FullUrl {:?}, opening directly", &url);
            if cfg.preferred_browser.is_empty() {
                open::that(url.as_str())?;
            } else {
                open::with(url.as_str(), cfg.preferred_browser)?;
            }
        }
        InputType::FuzzyPattern(_segments) => {
            anyhow::bail!("Opening links from a fuzzy pattern is not implemented yet!")
        }
    }
    Ok(())
}
