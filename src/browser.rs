use anyhow::Result;
use log::debug;
use open;
use std::time::SystemTime;

use crate::database::Database;
use crate::url_classify::{InputType, classify_input};

pub trait BrowserOpener {
    fn open(&self, url: &str, browser: Option<&str>) -> std::io::Result<()>;
}

pub struct SystemBrowserOpener;
impl BrowserOpener for SystemBrowserOpener {
    fn open(&self, url: &str, browser: Option<&str>) -> std::io::Result<()> {
        open_url(url, browser)
    }
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

#[cfg(test)]
pub struct MockBrowserOpener {
    pub captured: std::rc::Rc<std::cell::RefCell<Option<(String, Option<String>)>>>,
}
#[cfg(test)]
impl BrowserOpener for MockBrowserOpener {
    fn open(&self, url: &str, browser: Option<&str>) -> std::io::Result<()> {
        *self.captured.borrow_mut() = Some((url.to_string(), browser.map(String::from)));
        Ok(())
    }
}

pub fn open_address_impl(
    opener: &dyn BrowserOpener,
    db: &mut dyn Database,
    address: &str,
    preferred_browser: Option<&str>,
) -> Result<()> {
    if address.is_empty() {
        anyhow::bail!("provided address must be a non-empty string");
    }

    match classify_input(address) {
        InputType::FullUrl(url) => {
            db.add_visit(url.as_str(), SystemTime::now())?;
            opener.open(url.as_str(), preferred_browser)?;
            Ok(())
        }
        InputType::FuzzyPattern(segments) => match db.get_best_match(&segments)? {
            Some(best_match) => {
                db.add_visit(&best_match, SystemTime::now())?;
                opener.open(best_match.as_str(), preferred_browser)?;
                Ok(())
            }
            None => {
                anyhow::bail!("No matching URL found in history");
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::SqliteDatabase;
    use assert_fs::TempDir;
    use std::cell::RefCell;
    use std::rc::Rc;
    fn create_mock() -> (
        MockBrowserOpener,
        Rc<RefCell<Option<(String, Option<String>)>>>,
    ) {
        let captured = Rc::new(RefCell::new(None));
        let mock = MockBrowserOpener {
            captured: captured.clone(),
        };
        (mock, captured)
    }

    fn create_temp_db() -> (TempDir, SqliteDatabase) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SqliteDatabase::open_at(&db_path).unwrap();
        (temp_dir, db)
    }
    #[test]
    fn empty_address_returns_error() {
        let (mock, _) = create_mock();
        let (_temp_dir, mut db) = create_temp_db();
        let result = open_address_impl(&mock, &mut db, "", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-empty"));
    }

    #[test]
    fn full_url_with_https_scheme_default_browser() {
        let (mock, captured) = create_mock();
        let (_temp_dir, mut db) = create_temp_db();

        open_address_impl(&mock, &mut db, "https://github.com/rust-lang/rust", None).unwrap();

        assert_eq!(
            *captured.borrow(),
            Some(("https://github.com/rust-lang/rust".to_string(), None))
        );
    }

    #[test]
    fn domain_without_scheme_adds_https() {
        let (mock, captured) = create_mock();
        let (_temp_dir, mut db) = create_temp_db();

        open_address_impl(&mock, &mut db, "github.com/rust-lang", None).unwrap();

        assert_eq!(
            *captured.borrow(),
            Some(("https://github.com/rust-lang".to_string(), None))
        );
    }

    #[test]
    fn localhost_with_port_adds_http() {
        let (mock, captured) = create_mock();
        let (_temp_dir, mut db) = create_temp_db();

        open_address_impl(&mock, &mut db, "localhost:8080/api", None).unwrap();

        assert_eq!(
            *captured.borrow(),
            Some(("http://localhost:8080/api".to_string(), None))
        );
    }

    #[test]
    fn full_url_with_preferred_browser() {
        let (mock, captured) = create_mock();
        let (_temp_dir, mut db) = create_temp_db();

        open_address_impl(&mock, &mut db, "https://github.com", Some("firefox")).unwrap();

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
        let (mock, captured) = create_mock();
        let (_temp_dir, mut db) = create_temp_db();

        open_address_impl(&mock, &mut db, "github.com/rust", Some("safari")).unwrap();

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
        let (mock, captured) = create_mock();
        let (_temp_dir, mut db) = create_temp_db();

        open_address_impl(&mock, &mut db, "example.com/search?q=rust&page=2", None).unwrap();

        assert_eq!(
            *captured.borrow(),
            Some(("https://example.com/search?q=rust&page=2".to_string(), None))
        );
    }

    #[test]
    fn preserves_fragment() {
        let (mock, captured) = create_mock();
        let (_temp_dir, mut db) = create_temp_db();

        open_address_impl(&mock, &mut db, "example.com/page#section", None).unwrap();

        assert_eq!(
            *captured.borrow(),
            Some(("https://example.com/page#section".to_string(), None))
        );
    }

    #[test]
    fn preserves_query_and_fragment() {
        let (mock, captured) = create_mock();
        let (_temp_dir, mut db) = create_temp_db();

        open_address_impl(&mock, &mut db, "github.com/search?q=rust#results", None).unwrap();

        assert_eq!(
            *captured.borrow(),
            Some(("https://github.com/search?q=rust#results".to_string(), None))
        );
    }
}
