//! Integration tests for cancelling searches

#[path = "support/grep.rs"]
pub mod grep_support;
#[path = "support/search.rs"]
pub mod search_support;

use fdr_core::{GrepConfig, SearchConfig, SearchError};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn search_with_cancel_refuses_to_start_when_already_cancelled() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    File::create(temp_path.join("file.txt")).expect("should create file");

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        ..Default::default()
    };

    let result = search_support::collect(&config, true);
    assert!(
        matches!(result, Err(SearchError::Cancelled)),
        "cancelled search should return SearchError::Cancelled, got {result:?}"
    );
}

#[test]
fn grep_with_cancel_refuses_to_start_when_already_cancelled() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    let mut handle = File::create(temp_path.join("file.txt")).expect("should create file");
    writeln!(handle, "needle").expect("should write file");

    let config = GrepConfig {
        pattern: "needle".to_string(),
        search: SearchConfig {
            paths: vec![PathBuf::from(temp_path)],
            ..Default::default()
        },
        ..Default::default()
    };

    let result = grep_support::collect(&config, true);
    assert!(
        matches!(result, Err(SearchError::Cancelled)),
        "cancelled grep should return SearchError::Cancelled, got {result:?}"
    );
}

#[test]
fn search_with_cancel_completes_when_not_cancelled() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    File::create(temp_path.join("file.txt")).expect("should create file");

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        ..Default::default()
    };

    let results =
        search_support::collect(&config, false).expect("uncancelled search should succeed");
    assert_eq!(results.len(), 1, "uncancelled search should find the file");
}
