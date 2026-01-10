use anyhow::{Context, Result};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use log::{debug, info};
use rusqlite::{Connection, params};
use std::path::PathBuf;
use std::time::SystemTime;
use url::Url;

pub trait Database {
    fn add_visit(&mut self, url: &str, timestamp: SystemTime) -> Result<()>;
    fn fuzzy_match(&self, pattern: &[String]) -> Result<Vec<(String, f64, i64)>>;
    fn get_best_match(&self, pattern: &[String]) -> Result<Option<String>>;
    fn get_highest_usage_urls(&self, size: u16) -> Result<Vec<(String, f64, i64)>>;
    fn prune_by_age(&mut self, older_than_secs: i64) -> Result<usize>;
    fn prune_by_url_pattern(&mut self, pattern: &str) -> Result<usize>;
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

        debug!("Connected to Database");
        let db = Self { conn };
        db.initialize_schema()?;
        Ok(db)
    }

    fn initialize_schema(&self) -> Result<()> {
        debug!("Initializing Database schema");
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
        let app_dir = data_dir.join("otot");
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

        info!("Recording visit for {:?}", url);

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

    fn fuzzy_match(&self, pattern: &[String]) -> Result<Vec<(String, f64, i64)>> {
        if pattern.is_empty() {
            return Ok(vec![]);
        }

        let last_segment = pattern.last().unwrap();
        let mut stmt = self.conn.prepare(
            "SELECT full_url, segments, score, last_accessed
                 FROM urls
                 WHERE last_segment = ?1 COLLATE NOCASE",
        )?;

        debug!("Querying for match on last-segment: {:?}", last_segment);

        let rows = stmt.query_map([last_segment], |row| {
            Ok((
                row.get::<_, String>(0)?, // full_url
                row.get::<_, String>(1)?, // segments JSON
                row.get::<_, f64>(2)?,    // score
                row.get::<_, i64>(3)?,    // last_accessed
            ))
        })?;

        let mut matches: Vec<(String, f64, i64)> = Vec::new();
        let mut row_count: u64 = 0;

        for row in rows {
            row_count += 1;
            let (url, segments_json, score, last_accessed) = row?;

            let url_segments: Vec<String> = serde_json::from_str(&segments_json)?;

            if does_pattern_match_segments(&url_segments, pattern) {
                let frecency = calculate_frecency(score, last_accessed);
                debug!(
                    "Matched: {} (score: {}, frecency: {:.2})",
                    url, score, frecency
                );
                matches.push((url, frecency, last_accessed));
            }
        }

        debug!("{:?} records matched on last segment", row_count);
        if matches.is_empty() {
            info!("No matches found for pattern {:?}", pattern);
        } else {
            info!(
                "Found {} match(es) for pattern {:?}",
                matches.len(),
                pattern
            );
        }

        matches.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        Ok(matches)
    }

    fn get_best_match(&self, pattern: &[String]) -> Result<Option<String>> {
        Ok(self
            .fuzzy_match(pattern)?
            .into_iter()
            .next()
            .map(|(s, _, _)| s))
    }

    fn get_highest_usage_urls(&self, size: u16) -> Result<Vec<(String, f64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT full_url, score, last_accessed
                 FROM urls
                 ORDER BY score DESC
                 LIMIT ?1",
        )?;

        let rows = stmt.query_map([size], |row| {
            Ok((
                row.get::<_, String>(0)?, // full_url
                row.get::<_, f64>(1)?,    // score
                row.get::<_, i64>(2)?,    // last_accessed
            ))
        })?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to collect highest usage URLs")
    }

    fn prune_by_age(&mut self, older_than_secs: i64) -> Result<usize> {
        let cutoff_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs() as i64
            - older_than_secs;

        let deleted = self
            .conn
            .execute("DELETE FROM urls WHERE last_accessed < ?1", [cutoff_time])?;

        Ok(deleted)
    }

    fn prune_by_url_pattern(&mut self, pattern: &str) -> Result<usize> {
        // For now, not going to add the SQLite regex plugin.  Usage should be pretty simple - beginning, end markers, etc.
        let like_pattern = convert_pattern_to_like(pattern)?;

        let deleted = self
            .conn
            .execute("DELETE FROM urls WHERE full_url LIKE ?1", [like_pattern])?;

        Ok(deleted)
    }
}

fn convert_pattern_to_like(pattern: &str) -> Result<String> {
    let unescaped = pattern.replace(r"\.", ".");

    let like_pattern = if unescaped.starts_with('^') && unescaped.ends_with('$') {
        unescaped
            .trim_start_matches('^')
            .trim_end_matches('$')
            .to_string()
    } else if unescaped.starts_with('^') {
        format!("{}%", unescaped.trim_start_matches('^'))
    } else if unescaped.ends_with('$') {
        format!("%{}", unescaped.trim_end_matches('$'))
    } else {
        format!("%{}%", unescaped)
    };

    Ok(like_pattern)
}

fn extract_segments(url_str: &str) -> Result<Vec<String>> {
    let url = Url::parse(url_str).context("Failed to parse URL")?;

    let mut segments: Vec<String> = Vec::new();

    if let Some(domain) = url.domain() {
        segments.push(domain.to_lowercase());
    }

    if let Some(path_segments) = url.path_segments() {
        segments.extend(
            path_segments
                .filter(|s| !s.is_empty())
                .map(|s| s.to_lowercase()),
        );
    }

    debug!("Extracted segments from {:?}: {:?}", url.as_str(), segments);

    Ok(segments)
}

fn get_last_segment(segments: &[String]) -> Option<String> {
    segments.last().cloned()
}

fn does_pattern_match_segments(url_segments: &[String], pattern: &[String]) -> bool {
    if pattern.is_empty() {
        return true;
    }

    let matcher = SkimMatcherV2::default();

    if let (Some(pattern_first), Some(url_first)) = (pattern.first(), url_segments.first()) {
        let first_match = if pattern_first.len() < 3 {
            pattern_first.eq_ignore_ascii_case(url_first)
        } else {
            matcher.fuzzy_match(url_first, pattern_first).is_some()
        };

        if !first_match {
            return false;
        }
    }

    if let (Some(pattern_last), Some(url_last)) = (pattern.last(), url_segments.last()) {
        let last_match = if pattern_last.len() < 3 {
            pattern_last.eq_ignore_ascii_case(url_last)
        } else {
            matcher.fuzzy_match(url_last, pattern_last).is_some()
        };

        if !last_match {
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

fn calculate_frecency(score: f64, last_accessed: i64) -> f64 {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let seconds_ago = now - last_accessed;

    let multiplier = if seconds_ago < 3600 {
        4.0
    } else if seconds_ago < 86400 {
        2.0
    } else if seconds_ago < 604800 {
        0.5
    } else {
        0.25
    };

    score * multiplier
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
        assert_eq!(result, vec!["github", "rust-lang", "rust"]);
    }
    #[test]
    fn extract_segments_multiple_path_segments() {
        let result =
            extract_segments("https://github.com/microsoft/typescript/issues/123").unwrap();
        assert_eq!(
            result,
            vec!["github", "microsoft", "typescript", "issues", "123"]
        );
    }
    #[test]
    fn extract_segments_root_only_no_path() {
        let result = extract_segments("https://github.com").unwrap();
        assert_eq!(result, vec!["github"]);
    }
    #[test]
    fn extract_segments_trailing_slash() {
        let result = extract_segments("https://github.com/rust-lang/rust/").unwrap();
        assert_eq!(result, vec!["github", "rust-lang", "rust"]);
    }
    // Category 2: Case Normalization
    #[test]
    fn extract_segments_mixed_case_normalized() {
        let result = extract_segments("https://GitHub.COM/Rust-Lang/RUST").unwrap();
        assert_eq!(result, vec!["github", "rust-lang", "rust"]);
    }
    #[test]
    fn extract_segments_already_lowercase() {
        let result = extract_segments("https://github.com/rust-lang/rust").unwrap();
        assert_eq!(result, vec!["github", "rust-lang", "rust"]);
    }
    // Category 3: Query Parameters and Fragments
    #[test]
    fn extract_segments_with_query_parameters() {
        let result = extract_segments("https://github.com/search?q=rust").unwrap();
        assert_eq!(result, vec!["github", "search"]);
    }
    #[test]
    fn extract_segments_with_fragment() {
        let result = extract_segments("https://github.com/rust-lang/rust#readme").unwrap();
        assert_eq!(result, vec!["github", "rust-lang", "rust"]);
    }
    #[test]
    fn extract_segments_with_query_and_fragment() {
        let result = extract_segments("https://github.com/search?q=rust#results").unwrap();
        assert_eq!(result, vec!["github", "search"]);
    }
    // Category 4: Different Schemes
    #[test]
    fn extract_segments_http_scheme() {
        let result = extract_segments("http://example.com/foo/bar").unwrap();
        assert_eq!(result, vec!["example", "foo", "bar"]);
    }
    #[test]
    fn extract_segments_https_scheme() {
        let result = extract_segments("https://example.com/foo/bar").unwrap();
        assert_eq!(result, vec!["example", "foo", "bar"]);
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

    // Tests for add_visit and database operations
    use assert_fs::TempDir;
    fn create_test_db() -> (TempDir, SqliteDatabase) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SqliteDatabase::open_at(&db_path).unwrap();
        (temp_dir, db)
    }
    #[test]
    fn add_visit_creates_new_entry() {
        let (_temp_dir, mut db) = create_test_db();

        let url = "https://github.com/rust-lang/rust";
        let timestamp = SystemTime::now();

        db.add_visit(url, timestamp).unwrap();

        // Verify entry was created by querying directly
        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM urls WHERE full_url = ?1",
                [url],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(count, 1);
    }
    #[test]
    fn add_visit_increments_score_on_duplicate() {
        let (_temp_dir, mut db) = create_test_db();

        let url = "https://github.com/rust-lang/rust";

        // First visit
        db.add_visit(url, SystemTime::now()).unwrap();

        // Second visit
        db.add_visit(url, SystemTime::now()).unwrap();

        // Third visit
        db.add_visit(url, SystemTime::now()).unwrap();

        // Verify score incremented
        let score: f64 = db
            .conn
            .query_row("SELECT score FROM urls WHERE full_url = ?1", [url], |row| {
                row.get(0)
            })
            .unwrap();

        assert_eq!(score, 3.0);
    }
    #[test]
    fn add_visit_updates_last_accessed() {
        let (_temp_dir, mut db) = create_test_db();

        let url = "https://github.com/rust-lang/rust";

        let first_time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1000);
        db.add_visit(url, first_time).unwrap();

        let second_time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(2000);
        db.add_visit(url, second_time).unwrap();

        // Verify last_accessed was updated
        let last_accessed: i64 = db
            .conn
            .query_row(
                "SELECT last_accessed FROM urls WHERE full_url = ?1",
                [url],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(last_accessed, 2000);
    }
    #[test]
    fn add_visit_stores_segments_correctly() {
        let (_temp_dir, mut db) = create_test_db();

        let url = "https://github.com/rust-lang/rust/issues";
        db.add_visit(url, SystemTime::now()).unwrap();

        // Verify segments stored as JSON
        let segments_json: String = db
            .conn
            .query_row(
                "SELECT segments FROM urls WHERE full_url = ?1",
                [url],
                |row| row.get(0),
            )
            .unwrap();

        let segments: Vec<String> = serde_json::from_str(&segments_json).unwrap();
        assert_eq!(segments, vec!["github", "rust-lang", "rust", "issues"]);
    }
    #[test]
    fn add_visit_stores_last_segment_correctly() {
        let (_temp_dir, mut db) = create_test_db();

        let url = "https://github.com/rust-lang/rust/issues";
        db.add_visit(url, SystemTime::now()).unwrap();

        // Verify last segment
        let last_segment: String = db
            .conn
            .query_row(
                "SELECT last_segment FROM urls WHERE full_url = ?1",
                [url],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(last_segment, "issues");
    }
    #[test]
    fn add_visit_normalizes_segments_to_lowercase() {
        let (_temp_dir, mut db) = create_test_db();

        let url = "https://GitHub.com/Rust-Lang/RUST";
        db.add_visit(url, SystemTime::now()).unwrap();

        let segments_json: String = db
            .conn
            .query_row(
                "SELECT segments FROM urls WHERE full_url = ?1",
                [url],
                |row| row.get(0),
            )
            .unwrap();

        let segments: Vec<String> = serde_json::from_str(&segments_json).unwrap();
        assert_eq!(segments, vec!["github", "rust-lang", "rust"]);
    }
    #[test]
    fn add_visit_handles_url_with_no_path() {
        let (_temp_dir, mut db) = create_test_db();

        let url = "https://github.com";
        db.add_visit(url, SystemTime::now()).unwrap();

        let segments_json: String = db
            .conn
            .query_row(
                "SELECT segments FROM urls WHERE full_url = ?1",
                [url],
                |row| row.get(0),
            )
            .unwrap();

        let segments: Vec<String> = serde_json::from_str(&segments_json).unwrap();
        assert_eq!(segments, vec!["github"]);

        let last_segment: String = db
            .conn
            .query_row(
                "SELECT last_segment FROM urls WHERE full_url = ?1",
                [url],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(last_segment, "github");
    }
    #[test]
    fn add_visit_multiple_different_urls() {
        let (_temp_dir, mut db) = create_test_db();

        db.add_visit("https://github.com/rust-lang/rust", SystemTime::now())
            .unwrap();
        db.add_visit("https://github.com/microsoft/typescript", SystemTime::now())
            .unwrap();
        db.add_visit("https://gitlab.com/foo/bar", SystemTime::now())
            .unwrap();

        // Verify all three URLs exist
        let count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM urls", [], |row| row.get(0))
            .unwrap();

        assert_eq!(count, 3);
    }

    // fuzzy_match
    #[test]
    fn fuzzy_match_returns_matching_urls() {
        let (_temp_dir, mut db) = create_test_db();

        // Add some URLs - make sure they end with "rust"
        db.add_visit("https://github.com/rust-lang/rust", SystemTime::now())
            .unwrap();
        db.add_visit("https://github.com/microsoft/rust", SystemTime::now())
            .unwrap();
        db.add_visit("https://gitlab.com/rust-lang/rust", SystemTime::now())
            .unwrap();

        // Search for pattern ending in "rust"
        let matches = db
            .fuzzy_match(&["github".to_string(), "rust".to_string()])
            .unwrap();

        // Should match the two github URLs (not gitlab, because first segment doesn't match)
        assert_eq!(matches.len(), 2);
        assert!(matches.iter().any(|(u, _, _)| u.contains("rust-lang")));
        assert!(matches.iter().any(|(u, _, _)| u.contains("microsoft")));
    }
    #[test]
    fn fuzzy_match_respects_segment_order() {
        let (_temp_dir, mut db) = create_test_db();

        db.add_visit("https://github.com/rust-lang/rust", SystemTime::now())
            .unwrap();
        db.add_visit("https://github.com/rust/issues", SystemTime::now())
            .unwrap(); // Different structure

        // Pattern: github -> rust-lang -> rust
        let matches = db
            .fuzzy_match(&[
                "github".to_string(),
                "rust-lang".to_string(),
                "rust".to_string(),
            ])
            .unwrap();

        // Should only match the first URL
        assert_eq!(matches.len(), 1);
        let (match_url, _, _) = &matches[0];
        assert_eq!(match_url, "https://github.com/rust-lang/rust");
    }
    #[test]
    fn fuzzy_match_sorts_by_frecency() {
        let (_temp_dir, mut db) = create_test_db();

        let old_time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1000);
        let recent_time = SystemTime::now();

        // Add URL visited long ago with high score - ending in "rust"
        db.add_visit("https://github.com/old/rust", old_time)
            .unwrap();
        db.add_visit("https://github.com/old/rust", old_time)
            .unwrap();
        db.add_visit("https://github.com/old/rust", old_time)
            .unwrap();

        // Add URL visited recently with lower score - also ending in "rust"
        db.add_visit("https://github.com/new/rust", recent_time)
            .unwrap();

        let matches = db
            .fuzzy_match(&["github".to_string(), "rust".to_string()])
            .unwrap();

        // Recent URL should come first due to recency boost
        let (match_url, _, _) = &matches[0];
        assert_eq!(match_url, "https://github.com/new/rust");
    }
    #[test]
    fn fuzzy_match_returns_empty_for_no_matches() {
        let (_temp_dir, mut db) = create_test_db();

        db.add_visit("https://github.com/rust-lang/rust", SystemTime::now())
            .unwrap();

        let matches = db
            .fuzzy_match(&["gitlab".to_string(), "foo".to_string()])
            .unwrap();

        assert_eq!(matches.len(), 0);
    }

    // get_highest_usage_urls
    #[test]
    fn get_highest_usage_urls_returns_top_urls_by_score() {
        let (_temp_dir, mut db) = create_test_db();
        db.add_visit("https://github.com/low", SystemTime::now())
            .unwrap();
        db.add_visit("https://github.com/high", SystemTime::now())
            .unwrap();
        db.add_visit("https://github.com/high", SystemTime::now())
            .unwrap();
        db.add_visit("https://github.com/high", SystemTime::now())
            .unwrap();

        let results = db.get_highest_usage_urls(5).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "https://github.com/high");
        assert_eq!(results[0].1, 3.0);
    }
    #[test]
    fn get_highest_usage_urls_respects_limit() {
        let (_temp_dir, mut db) = create_test_db();
        db.add_visit("https://example.com/1", SystemTime::now())
            .unwrap();
        db.add_visit("https://example.com/2", SystemTime::now())
            .unwrap();
        db.add_visit("https://example.com/3", SystemTime::now())
            .unwrap();

        let results = db.get_highest_usage_urls(2).unwrap();

        assert_eq!(results.len(), 2);
    }
    #[test]
    fn get_highest_usage_urls_returns_empty_for_empty_db() {
        let (_temp_dir, db) = create_test_db();

        let results = db.get_highest_usage_urls(10).unwrap();

        assert_eq!(results.len(), 0);
    }

    #[test]
    fn prune_by_age_removes_old_urls() {
        let (_temp_dir, mut db) = create_test_db();
        let old_time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1000);
        let recent_time = SystemTime::now();
        db.add_visit("https://github.com/old", old_time).unwrap();
        db.add_visit("https://github.com/recent", recent_time)
            .unwrap();

        let deleted = db.prune_by_age(3600).unwrap();
        assert_eq!(deleted, 1);
        // Verify the old URL is gone and recent one remains
        let count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM urls", [], |row| row.get(0))
            .unwrap();

        assert_eq!(count, 1);
        let remaining_url: String = db
            .conn
            .query_row("SELECT full_url FROM urls", [], |row| row.get(0))
            .unwrap();

        assert_eq!(remaining_url, "https://github.com/recent");
    }
    #[test]
    fn prune_by_age_returns_zero_when_no_matches() {
        let (_temp_dir, mut db) = create_test_db();
        db.add_visit("https://github.com/recent", SystemTime::now())
            .unwrap();

        let deleted = db.prune_by_age(31536000).unwrap();
        assert_eq!(deleted, 0);
    }
    #[test]
    fn prune_by_url_pattern_removes_matching_urls() {
        let (_temp_dir, mut db) = create_test_db();
        db.add_visit("https://github.com/rust-lang/rust", SystemTime::now())
            .unwrap();
        db.add_visit("https://github.com/microsoft/typescript", SystemTime::now())
            .unwrap();
        db.add_visit("https://gitlab.com/foo/bar", SystemTime::now())
            .unwrap();
        let deleted = db.prune_by_url_pattern("github.com").unwrap();
        assert_eq!(deleted, 2);

        let count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM urls", [], |row| row.get(0))
            .unwrap();

        assert_eq!(count, 1);
        let remaining_url: String = db
            .conn
            .query_row("SELECT full_url FROM urls", [], |row| row.get(0))
            .unwrap();

        assert_eq!(remaining_url, "https://gitlab.com/foo/bar");
    }
    #[test]
    fn prune_by_url_pattern_returns_zero_when_no_matches() {
        let (_temp_dir, mut db) = create_test_db();
        db.add_visit("https://github.com/rust", SystemTime::now())
            .unwrap();

        let deleted = db.prune_by_url_pattern("gitlab.com").unwrap();
        assert_eq!(deleted, 0);
    }
    #[test]
    fn prune_by_url_pattern_matches_partial_strings() {
        let (_temp_dir, mut db) = create_test_db();
        db.add_visit("https://github.com/rust-lang/rust", SystemTime::now())
            .unwrap();
        db.add_visit("https://github.com/microsoft/rust", SystemTime::now())
            .unwrap();
        db.add_visit("https://github.com/other/project", SystemTime::now())
            .unwrap();

        let deleted = db.prune_by_url_pattern("rust").unwrap();
        assert_eq!(deleted, 2);
    }
    #[test]
    fn prune_by_url_pattern_exact_match() {
        let (_temp_dir, mut db) = create_test_db();
        db.add_visit("https://github.com/", SystemTime::now())
            .unwrap();
        db.add_visit("https://github.com/rust", SystemTime::now())
            .unwrap();

        let deleted = db.prune_by_url_pattern("^https://github\\.com/$").unwrap();
        assert_eq!(deleted, 1);
    }
    #[test]
    fn prune_by_url_pattern_prefix_match() {
        let (_temp_dir, mut db) = create_test_db();
        db.add_visit("https://github.com/rust", SystemTime::now())
            .unwrap();
        db.add_visit("https://github.com/microsoft", SystemTime::now())
            .unwrap();
        db.add_visit("https://gitlab.com/foo", SystemTime::now())
            .unwrap();

        let deleted = db.prune_by_url_pattern("^https://github\\.com/").unwrap();
        assert_eq!(deleted, 2);
    }
    #[test]
    fn prune_by_url_pattern_suffix_match() {
        let (_temp_dir, mut db) = create_test_db();
        db.add_visit("https://github.com/rust-lang/rust", SystemTime::now())
            .unwrap();
        db.add_visit("https://github.com/microsoft/rust", SystemTime::now())
            .unwrap();
        db.add_visit("https://github.com/other/project", SystemTime::now())
            .unwrap();

        let deleted = db.prune_by_url_pattern("/rust$").unwrap();
        assert_eq!(deleted, 2);
    }
    #[test]
    fn prune_by_url_pattern_contains_match() {
        let (_temp_dir, mut db) = create_test_db();
        db.add_visit("https://github.com/rust-lang/rust", SystemTime::now())
            .unwrap();
        db.add_visit("https://github.com/microsoft/typescript", SystemTime::now())
            .unwrap();
        db.add_visit("https://gitlab.com/foo/bar", SystemTime::now())
            .unwrap();

        let deleted = db.prune_by_url_pattern("github\\.com").unwrap();
        assert_eq!(deleted, 2);
    }

    #[test]
    fn prune_by_age_with_empty_database() {
        let (_temp_dir, mut db) = create_test_db();
        let deleted = db.prune_by_age(86400).unwrap();
        assert_eq!(deleted, 0);
    }
    #[test]
    fn prune_by_url_pattern_with_empty_database() {
        let (_temp_dir, mut db) = create_test_db();
        let deleted = db.prune_by_url_pattern("github.com").unwrap();
        assert_eq!(deleted, 0);
    }
}
