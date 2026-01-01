use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use serde_json;
use std::path::PathBuf;
use std::time::SystemTime;
use url::Url;

pub trait Database {
    fn add_visit(&mut self, url: &str, timestamp: SystemTime) -> Result<()>;
    fn fuzzy_match(&self, pattern: &[String]) -> Result<Vec<String>>;
    fn get_best_match(&self, pattern: &[String]) -> Result<Option<String>>;
}

pub struct SqliteDatabase {
    conn: Connection,
}

impl SqliteDatabase {
    pub fn open() -> Result<Self> {
        let db_path = Self::get_db_path()?;
        Self::open_at(&db_path)
    }

    pub fn open_at(path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(path).context("Failed to open database")?;

        let db = Self { conn };
        db.initialize_schema()?;
        Ok(db)
    }

    fn initialize_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS urls (
                id INTEGER PRIMARY KEY,
                full_url TEXT NOT NULL UNIQUE,
                segments TEXT NOT NULL,
                last_segment TEXT NOT NULL,
                score REAL NOT NULL DEFAULT 1.0,
                last_accessed INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_urls_last_segment
                ON urls(last_segment COLLATE NOCASE);",
        )?;
        Ok(())
    }

    fn get_db_path() -> Result<PathBuf> {
        let data_dir = dirs::data_local_dir().context("Could not find local data directory")?;
        let app_dir = data_dir.join("zurl");
        std::fs::create_dir_all(&app_dir).context("Failed to create application directory")?;

        Ok(app_dir.join("history.db"))
    }
}

impl Database for SqliteDatabase {
    fn add_visit(&mut self, url: &str, timestamp: SystemTime) -> Result<()> {
        let segments = extract_segments(url)?;
        let last_segment = get_last_segment(&segments).unwrap_or_default();
        let segments_json = serde_json::to_string(&segments)?;
        let timestamp_secs = timestamp.duration_since(SystemTime::UNIX_EPOCH)?.as_secs() as i64;

        self.conn.execute(
            "INSERT INTO urls (full_url, segments, last_segment, score, last_accessed)
                  VALUES (?1, ?2, ?3, 1.0, ?4)
                  ON CONFLICT(full_url) DO UPDATE SET
                      score = score + 1.0,
                      last_accessed = excluded.last_accessed",
            params![url, segments_json, last_segment, timestamp_secs],
        )?;

        Ok(())
    }

    fn fuzzy_match(&self, pattern: &[String]) -> Result<Vec<String>> {
        todo!()
    }
    fn get_best_match(&self, pattern: &[String]) -> Result<Option<String>> {
        todo!()
    }
}

fn extract_segments(url_str: &str) -> Result<Vec<String>> {
    let url = Url::parse(url_str).context("Failed to parse URL")?;

    let segments: Vec<String> = url
        .path_segments()
        .map(|segments| {
            segments
                .filter(|s| !s.is_empty())
                .map(|s| s.to_lowercase())
                .collect()
        })
        .unwrap_or_default();

    Ok(segments)
}

fn get_last_segment(segments: &[String]) -> Option<String> {
    segments.last().cloned()
}

fn does_pattern_match_segments(url_segments: &[String], pattern: &[String]) -> bool {
    if pattern.is_empty() {
        return true;
    }

    if let (Some(pattern_first), Some(url_first)) = (pattern.first(), url_segments.first()) {
        if pattern_first != url_first {
            return false;
        }
    }

    if let (Some(pattern_last), Some(url_last)) = (pattern.last(), url_segments.last()) {
        if pattern_last != url_last {
            return false;
        }
    }

    let mut url_idx = 0;
    for pattern_seg in pattern {
        let found = url_segments[url_idx..]
            .iter()
            .position(|url_seg| url_seg == pattern_seg);

        match found {
            Some(offset) => {
                url_idx += offset + 1;
            }
            None => return false,
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_strings(slice: &[&str]) -> Vec<String> {
        slice.iter().map(|s| s.to_string()).collect()
    }
    // does_pattern_match_segments

    // Category 1: First Segment Rule
    #[test]
    fn first_segment_matches() {
        let url_segments = to_strings(&["github", "rust", "issues"]);
        let pattern = to_strings(&["github", "issues"]);
        assert!(does_pattern_match_segments(&url_segments, &pattern));
    }
    #[test]
    fn first_segment_does_not_match() {
        let url_segments = to_strings(&["gitlab", "rust", "issues"]);
        let pattern = to_strings(&["github", "issues"]);
        assert!(!does_pattern_match_segments(&url_segments, &pattern));
    }
    // Category 2: Last Segment Rule
    #[test]
    fn last_segment_matches() {
        let url_segments = to_strings(&["github", "rust", "issues"]);
        let pattern = to_strings(&["github", "issues"]);
        assert!(does_pattern_match_segments(&url_segments, &pattern));
    }
    #[test]
    fn last_segment_does_not_match() {
        let url_segments = to_strings(&["github", "rust", "pulls"]);
        let pattern = to_strings(&["github", "issues"]);
        assert!(!does_pattern_match_segments(&url_segments, &pattern));
    }
    #[test]
    fn pattern_last_segment_appears_in_middle_of_url() {
        let url_segments = to_strings(&["github", "issues", "rust"]);
        let pattern = to_strings(&["github", "issues"]);
        assert!(!does_pattern_match_segments(&url_segments, &pattern));
    }
    // Category 3: Ordering with Gaps
    #[test]
    fn all_pattern_segments_present_in_order_with_gaps() {
        let url_segments = to_strings(&["github", "microsoft", "rust", "foo", "bar", "issues"]);
        let pattern = to_strings(&["github", "rust", "issues"]);
        assert!(does_pattern_match_segments(&url_segments, &pattern));
    }
    #[test]
    fn all_pattern_segments_present_but_out_of_order() {
        let url_segments = to_strings(&["github", "issues", "rust"]);
        let pattern = to_strings(&["github", "rust", "issues"]);
        assert!(!does_pattern_match_segments(&url_segments, &pattern));
    }
    #[test]
    fn pattern_segments_appear_multiple_times() {
        let url_segments = to_strings(&["github", "rust", "microsoft", "rust", "issues"]);
        let pattern = to_strings(&["github", "rust", "issues"]);
        assert!(does_pattern_match_segments(&url_segments, &pattern));
    }
    #[test]
    fn no_gaps_needed_consecutive_segments() {
        let url_segments = to_strings(&["github", "rust", "issues"]);
        let pattern = to_strings(&["github", "rust", "issues"]);
        assert!(does_pattern_match_segments(&url_segments, &pattern));
    }
    #[test]
    fn single_gap_between_segments() {
        let url_segments = to_strings(&["github", "foo", "issues"]);
        let pattern = to_strings(&["github", "issues"]);
        assert!(does_pattern_match_segments(&url_segments, &pattern));
    }
    // Category 4: Single Segment Patterns
    #[test]
    fn single_segment_pattern_matching_single_segment_url() {
        let url_segments = to_strings(&["github"]);
        let pattern = to_strings(&["github"]);
        assert!(does_pattern_match_segments(&url_segments, &pattern));
    }
    #[test]
    fn single_segment_pattern_multi_segment_url() {
        let url_segments = to_strings(&["github", "rust"]);
        let pattern = to_strings(&["github"]);
        assert!(!does_pattern_match_segments(&url_segments, &pattern));
    }
    #[test]
    fn single_segment_pattern_matches_both_first_and_last() {
        let url_segments = to_strings(&["github"]);
        let pattern = to_strings(&["github"]);
        assert!(does_pattern_match_segments(&url_segments, &pattern));
    }
    // Category 5: Missing Pattern Segments
    #[test]
    fn middle_pattern_segment_missing_from_url() {
        let url_segments = to_strings(&["github", "issues"]);
        let pattern = to_strings(&["github", "rust", "issues"]);
        assert!(!does_pattern_match_segments(&url_segments, &pattern));
    }
    #[test]
    fn pattern_longer_than_url() {
        let url_segments = to_strings(&["github", "rust"]);
        let pattern = to_strings(&["github", "foo", "bar", "rust"]);
        assert!(!does_pattern_match_segments(&url_segments, &pattern));
    }
    // Category 6: Edge Cases
    #[test]
    fn url_has_no_segments() {
        let url_segments: Vec<String> = vec![];
        let pattern = to_strings(&["github"]);
        assert!(!does_pattern_match_segments(&url_segments, &pattern));
    }

    // extract_segments

    // Category 1: Basic URL Parsing
    #[test]
    fn extract_segments_simple_url_with_path() {
        let result = extract_segments("https://github.com/rust-lang/rust").unwrap();
        assert_eq!(result, vec!["rust-lang", "rust"]);
    }
    #[test]
    fn extract_segments_multiple_path_segments() {
        let result =
            extract_segments("https://github.com/microsoft/typescript/issues/123").unwrap();
        assert_eq!(result, vec!["microsoft", "typescript", "issues", "123"]);
    }
    #[test]
    fn extract_segments_root_only_no_path() {
        let result = extract_segments("https://github.com").unwrap();
        assert_eq!(result, Vec::<String>::new());
    }
    #[test]
    fn extract_segments_trailing_slash() {
        let result = extract_segments("https://github.com/rust-lang/rust/").unwrap();
        assert_eq!(result, vec!["rust-lang", "rust"]);
    }
    // Category 2: Case Normalization
    #[test]
    fn extract_segments_mixed_case_normalized() {
        let result = extract_segments("https://GitHub.COM/Rust-Lang/RUST").unwrap();
        assert_eq!(result, vec!["rust-lang", "rust"]);
    }
    #[test]
    fn extract_segments_already_lowercase() {
        let result = extract_segments("https://github.com/rust-lang/rust").unwrap();
        assert_eq!(result, vec!["rust-lang", "rust"]);
    }
    // Category 3: Query Parameters and Fragments
    #[test]
    fn extract_segments_with_query_parameters() {
        let result = extract_segments("https://github.com/search?q=rust").unwrap();
        assert_eq!(result, vec!["search"]);
    }
    #[test]
    fn extract_segments_with_fragment() {
        let result = extract_segments("https://github.com/rust-lang/rust#readme").unwrap();
        assert_eq!(result, vec!["rust-lang", "rust"]);
    }
    #[test]
    fn extract_segments_with_query_and_fragment() {
        let result = extract_segments("https://github.com/search?q=rust#results").unwrap();
        assert_eq!(result, vec!["search"]);
    }
    // Category 4: Different Schemes
    #[test]
    fn extract_segments_http_scheme() {
        let result = extract_segments("http://example.com/foo/bar").unwrap();
        assert_eq!(result, vec!["foo", "bar"]);
    }
    #[test]
    fn extract_segments_https_scheme() {
        let result = extract_segments("https://example.com/foo/bar").unwrap();
        assert_eq!(result, vec!["foo", "bar"]);
    }
    // Category 5: Error Cases
    #[test]
    fn extract_segments_invalid_url() {
        let result = extract_segments("not-a-valid-url");
        assert!(result.is_err());
    }
    #[test]
    fn extract_segments_empty_string() {
        let result = extract_segments("");
        assert!(result.is_err());
    }
    #[test]
    fn extract_segments_no_scheme() {
        let result = extract_segments("github.com/rust-lang/rust");
        assert!(result.is_err());
    }
}
