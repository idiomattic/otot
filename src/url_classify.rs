use url::Url;

#[derive(Debug, PartialEq)]
pub enum InputType {
    FullUrl(Url),
    FuzzyPattern(Vec<String>),
}

pub fn classify_input(address: &str) -> InputType {
    if address.contains("://")
        && let Ok(url) = Url::parse(address)
    {
        return InputType::FullUrl(url);
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
        if url.host_str().is_some_and(|h| h.contains('.')) || url.port().is_some() {
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

#[cfg(test)]
mod classify_input_tests {
    use super::*;

    // Rule 1: Explicit Scheme (Fully-Qualified URLs)
    #[test]
    fn explicit_https_scheme() {
        let result = classify_input("https://github.com/rust-lang/rust");

        match result {
            InputType::FullUrl(url) => {
                assert_eq!(url.scheme(), "https");
                assert_eq!(url.host_str(), Some("github.com"));
                assert_eq!(url.path(), "/rust-lang/rust");
            }
            _ => panic!("Expected FullUrl variant"),
        }
    }
    #[test]
    fn explicit_http_scheme() {
        let result = classify_input("http://example.com/path");

        match result {
            InputType::FullUrl(url) => {
                assert_eq!(url.scheme(), "http");
                assert_eq!(url.host_str(), Some("example.com"));
                assert_eq!(url.path(), "/path");
            }
            _ => panic!("Expected FullUrl variant"),
        }
    }
    #[test]
    fn explicit_scheme_with_port() {
        let result = classify_input("http://localhost:8080/api");

        match result {
            InputType::FullUrl(url) => {
                assert_eq!(url.scheme(), "http");
                assert_eq!(url.host_str(), Some("localhost"));
                assert_eq!(url.port(), Some(8080));
                assert_eq!(url.path(), "/api");
            }
            _ => panic!("Expected FullUrl variant"),
        }
    }
    #[test]
    fn explicit_scheme_preserves_query_and_fragment() {
        let result = classify_input("https://example.com/search?q=rust#results");

        match result {
            InputType::FullUrl(url) => {
                assert_eq!(url.scheme(), "https");
                assert_eq!(url.host_str(), Some("example.com"));
                assert_eq!(url.path(), "/search");
                assert_eq!(url.query(), Some("q=rust"));
                assert_eq!(url.fragment(), Some("results"));
            }
            _ => panic!("Expected FullUrl variant"),
        }
    }
    #[test]
    fn explicit_scheme_normalizes_host_to_lowercase() {
        let result = classify_input("https://GitHub.COM/Rust-Lang");

        match result {
            InputType::FullUrl(url) => {
                assert_eq!(url.host_str(), Some("github.com"));
                assert_eq!(url.path(), "/Rust-Lang"); // path case preserved
            }
            _ => panic!("Expected FullUrl variant"),
        }
    }
    // Rule 2: Domain Without Scheme
    #[test]
    fn domain_without_scheme() {
        let result = classify_input("github.com/rust-lang/rust");

        match result {
            InputType::FullUrl(url) => {
                assert_eq!(url.scheme(), "https");
                assert_eq!(url.host_str(), Some("github.com"));
                assert_eq!(url.path(), "/rust-lang/rust");
            }
            _ => panic!("Expected FullUrl variant"),
        }
    }
    #[test]
    fn domain_without_scheme_with_port() {
        let result = classify_input("example.com:3000/path");

        match result {
            InputType::FullUrl(url) => {
                assert_eq!(url.scheme(), "http");
                assert_eq!(url.host_str(), Some("example.com"));
                assert_eq!(url.port(), Some(3000));
                assert_eq!(url.path(), "/path");
            }
            _ => panic!("Expected FullUrl variant"),
        }
    }
    #[test]
    fn domain_without_scheme_with_query_and_fragment() {
        let result = classify_input("github.com/search?q=rust#top");

        match result {
            InputType::FullUrl(url) => {
                assert_eq!(url.scheme(), "https");
                assert_eq!(url.host_str(), Some("github.com"));
                assert_eq!(url.path(), "/search");
                assert_eq!(url.query(), Some("q=rust"));
                assert_eq!(url.fragment(), Some("top"));
            }
            _ => panic!("Expected FullUrl variant"),
        }
    }
    #[test]
    fn domain_without_scheme_normalizes_to_lowercase() {
        let result = classify_input("GitHub.COM/Rust-Lang");

        match result {
            InputType::FullUrl(url) => {
                assert_eq!(url.scheme(), "https");
                assert_eq!(url.host_str(), Some("github.com"));
                assert_eq!(url.path(), "/Rust-Lang"); // path case preserved
            }
            _ => panic!("Expected FullUrl variant"),
        }
    }
    // Rule 3: Localhost with Port (Known to fail with current implementation)
    #[test]
    fn localhost_with_port_should_be_full_url() {
        let result = classify_input("localhost:8080");

        match result {
            InputType::FullUrl(url) => {
                assert_eq!(url.scheme(), "http");
                assert_eq!(url.host_str(), Some("localhost"));
                assert_eq!(url.port(), Some(8080));
            }
            _ => {
                panic!("Expected FullUrl variant, but current implementation returns FuzzyPattern")
            }
        }
    }
    #[test]
    fn ip_address_with_port_should_be_full_url() {
        let result = classify_input("192.168.1.1:3000/api");

        match result {
            InputType::FullUrl(url) => {
                assert_eq!(url.scheme(), "http");
                assert_eq!(url.host_str(), Some("192.168.1.1"));
                assert_eq!(url.port(), Some(3000));
                assert_eq!(url.path(), "/api");
            }
            _ => {
                panic!("Expected FullUrl variant, but current implementation returns FuzzyPattern")
            }
        }
    }
    // Rule 4: Fuzzy Patterns
    #[test]
    fn fuzzy_pattern_multiple_segments() {
        let result = classify_input("github/rust/issues");

        match result {
            InputType::FuzzyPattern(segments) => {
                assert_eq!(segments, vec!["github", "rust", "issues"]);
            }
            _ => panic!("Expected FuzzyPattern variant"),
        }
    }
    #[test]
    fn fuzzy_pattern_single_segment() {
        let result = classify_input("github");

        match result {
            InputType::FuzzyPattern(segments) => {
                assert_eq!(segments, vec!["github"]);
            }
            _ => panic!("Expected FuzzyPattern variant"),
        }
    }
    #[test]
    fn fuzzy_pattern_filters_empty_segments() {
        let result = classify_input("github//rust");

        match result {
            InputType::FuzzyPattern(segments) => {
                assert_eq!(segments, vec!["github", "rust"]);
            }
            _ => panic!("Expected FuzzyPattern variant"),
        }
    }
    #[test]
    fn fuzzy_pattern_discards_leading_slash() {
        let result = classify_input("/github/rust");

        match result {
            InputType::FuzzyPattern(segments) => {
                assert_eq!(segments, vec!["github", "rust"]);
            }
            _ => panic!("Expected FuzzyPattern variant"),
        }
    }
    #[test]
    fn fuzzy_pattern_discards_trailing_slash() {
        let result = classify_input("github/rust/");

        match result {
            InputType::FuzzyPattern(segments) => {
                assert_eq!(segments, vec!["github", "rust"]);
            }
            _ => panic!("Expected FuzzyPattern variant"),
        }
    }
    #[test]
    fn fuzzy_pattern_normalizes_to_lowercase() {
        let result = classify_input("GitHub/Rust/Issues");

        match result {
            InputType::FuzzyPattern(segments) => {
                assert_eq!(segments, vec!["github", "rust", "issues"]);
            }
            _ => panic!("Expected FuzzyPattern variant"),
        }
    }
}
