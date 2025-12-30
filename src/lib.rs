use anyhow::Result;
use clap::Subcommand;
use log::debug;
use open;
use url::Url;

#[derive(Debug, PartialEq)]
pub enum InputType {
    FullUrl(Url),
    FuzzyPattern(Vec<String>),
}

pub fn classify_input(address: &str) -> InputType {
    if address.contains("://") {
        if let Ok(url) = Url::parse(address) {
            return InputType::FullUrl(url);
        }
    }

    let inferred_scheme = if address.contains(':') {
        "http"
    } else {
        "https"
    };

    let with_scheme = format!("{}://{}", inferred_scheme, address);
    if let Ok(url) = Url::parse(&with_scheme) {
        // XXX: for now, we're assuming that, if the user didn't input a scheme, we can differentiate between a fuzzy pattern
        //   and a domain that just needs https prepended by the presence of a '.'
        if url.host_str().map_or(false, |h| h.contains('.')) || url.port().is_some() {
            return InputType::FullUrl(url);
        }
    }

    InputType::FuzzyPattern(
        address
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
            .collect(),
    )
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    Set { key: String, value: String },
    Get { key: String },
    Path,
}

fn open_url(url: &str, browser: Option<&str>) -> std::io::Result<()> {
    match browser {
        Some(b) => {
            debug!("Opening link with {:?}", &b);
            open::with(url, b)
        }
        None => {
            debug!("Opening link with default browser");
            open::that(url)
        }
    }
}

fn handle_open_impl<F>(address: &str, preferred_browser: Option<&str>, opener: F) -> Result<()>
where
    F: Fn(&str, Option<&str>) -> std::io::Result<()>,
{
    if address.is_empty() {
        anyhow::bail!("provided address must be a non-empty string");
    }

    match classify_input(address) {
        InputType::FullUrl(url) => {
            opener(url.as_str(), preferred_browser)?;
            Ok(())
        }
        InputType::FuzzyPattern(_segments) => {
            anyhow::bail!("Opening links from a fuzzy pattern is not implemented yet!")
        }
    }
}

pub fn handle_open(address: &str, preferred_browser: Option<&str>) -> anyhow::Result<()> {
    if address.is_empty() {
        anyhow::bail!("provided address must be a non-empty string");
    }

    let parsed = classify_input(&address);
    match parsed {
        InputType::FullUrl(url) => {
            debug!("Parsed FullUrl {:?}", &url);
            open_url(&url.as_str(), preferred_browser)?;
        }
        InputType::FuzzyPattern(segments) => {
            debug!("Parsed FuzzyPattern {:?}", &segments);
            anyhow::bail!("Opening links from a fuzzy pattern is not implemented yet!")
        }
    }
    Ok(())
}
pub fn handle_config(action: ConfigAction) -> Result<()> {
    debug!("Received config action: {:?}", &action);
    anyhow::bail!("Config command is not implemented yet!")
}
