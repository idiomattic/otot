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

pub fn open_url(url: &str, browser: Option<&str>) -> std::io::Result<()> {
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

pub fn open_address_impl<F>(opener: F, address: &str, preferred_browser: Option<&str>) -> Result<()>
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

pub fn handle_open_address(address: &str, preferred_browser: Option<&str>) -> anyhow::Result<()> {
    open_address_impl(open_url, address, preferred_browser)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    #[test]
    fn empty_address_returns_error() {
        let mock = |_: &str, _: Option<&str>| Ok(());
        let result = open_address_impl(mock, "", None);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-empty"));
    }
    #[test]
    fn full_url_with_https_scheme_default_browser() {
        let captured = Rc::new(RefCell::new(None));
        let captured_clone = captured.clone();

        let mock = move |url: &str, browser: Option<&str>| {
            *captured_clone.borrow_mut() = Some((url.to_string(), browser.map(String::from)));
            Ok(())
        };

        open_address_impl(mock, "https://github.com/rust-lang/rust", None).unwrap();

        assert_eq!(
            *captured.borrow(),
            Some(("https://github.com/rust-lang/rust".to_string(), None))
        );
    }
    #[test]
    fn domain_without_scheme_adds_https() {
        let captured = Rc::new(RefCell::new(None));
        let captured_clone = captured.clone();

        let mock = move |url: &str, browser: Option<&str>| {
            *captured_clone.borrow_mut() = Some((url.to_string(), browser.map(String::from)));
            Ok(())
        };

        open_address_impl(mock, "github.com/rust-lang", None).unwrap();

        assert_eq!(
            *captured.borrow(),
            Some(("https://github.com/rust-lang".to_string(), None))
        );
    }
    #[test]
    fn localhost_with_port_adds_http() {
        let captured = Rc::new(RefCell::new(None));
        let captured_clone = captured.clone();

        let mock = move |url: &str, browser: Option<&str>| {
            *captured_clone.borrow_mut() = Some((url.to_string(), browser.map(String::from)));
            Ok(())
        };

        open_address_impl(mock, "localhost:8080/api", None).unwrap();

        assert_eq!(
            *captured.borrow(),
            Some(("http://localhost:8080/api".to_string(), None))
        );
    }
    #[test]
    fn full_url_with_preferred_browser() {
        let captured = Rc::new(RefCell::new(None));
        let captured_clone = captured.clone();

        let mock = move |url: &str, browser: Option<&str>| {
            *captured_clone.borrow_mut() = Some((url.to_string(), browser.map(String::from)));
            Ok(())
        };

        open_address_impl(mock, "https://github.com", Some("firefox")).unwrap();

        assert_eq!(
            *captured.borrow(),
            Some((
                "https://github.com/".to_string(),
                Some("firefox".to_string())
            ))
        );
    }
    #[test]
    fn domain_without_scheme_with_preferred_browser() {
        let captured = Rc::new(RefCell::new(None));
        let captured_clone = captured.clone();

        let mock = move |url: &str, browser: Option<&str>| {
            *captured_clone.borrow_mut() = Some((url.to_string(), browser.map(String::from)));
            Ok(())
        };

        open_address_impl(mock, "github.com/rust", Some("safari")).unwrap();

        assert_eq!(
            *captured.borrow(),
            Some((
                "https://github.com/rust".to_string(),
                Some("safari".to_string())
            ))
        );
    }
    #[test]
    fn preserves_query_parameters() {
        let captured = Rc::new(RefCell::new(None));
        let captured_clone = captured.clone();

        let mock = move |url: &str, browser: Option<&str>| {
            *captured_clone.borrow_mut() = Some((url.to_string(), browser.map(String::from)));
            Ok(())
        };

        open_address_impl(mock, "example.com/search?q=rust&page=2", None).unwrap();

        assert_eq!(
            *captured.borrow(),
            Some(("https://example.com/search?q=rust&page=2".to_string(), None))
        );
    }
    #[test]
    fn preserves_fragment() {
        let captured = Rc::new(RefCell::new(None));
        let captured_clone = captured.clone();

        let mock = move |url: &str, browser: Option<&str>| {
            *captured_clone.borrow_mut() = Some((url.to_string(), browser.map(String::from)));
            Ok(())
        };

        open_address_impl(mock, "example.com/page#section", None).unwrap();

        assert_eq!(
            *captured.borrow(),
            Some(("https://example.com/page#section".to_string(), None))
        );
    }
    #[test]
    fn preserves_query_and_fragment() {
        let captured = Rc::new(RefCell::new(None));
        let captured_clone = captured.clone();

        let mock = move |url: &str, browser: Option<&str>| {
            *captured_clone.borrow_mut() = Some((url.to_string(), browser.map(String::from)));
            Ok(())
        };

        open_address_impl(mock, "github.com/search?q=rust#results", None).unwrap();

        assert_eq!(
            *captured.borrow(),
            Some(("https://github.com/search?q=rust#results".to_string(), None))
        );
    }
    #[test]
    fn fuzzy_pattern_returns_error() {
        let captured = Rc::new(RefCell::new(None));
        let captured_clone = captured.clone();

        let mock = move |url: &str, browser: Option<&str>| {
            *captured_clone.borrow_mut() = Some((url.to_string(), browser.map(String::from)));
            Ok(())
        };

        let result = open_address_impl(mock, "github/rust/issues", None);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not implemented"));

        // Verify opener was never called
        assert_eq!(*captured.borrow(), None);
    }
}

pub fn handle_config_action(action: ConfigAction) -> Result<()> {
    debug!("Received config action: {:?}", &action);
    anyhow::bail!("Config command is not implemented yet!")
}
