//! Integration tests for cancelling searches

use fdr_core::{GrepConfig, SearchConfig, SearchError, grep_with_cancel, search_with_cancel};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

#[test]
fn search_with_cancel_stops_when_cancelled() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    for index in 0..100 {
        let file = temp_path.join(format!("file_{index:03}.txt"));
        File::create(&file).expect("should create file");
    }

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        ..Default::default()
    };

    let cancel = AtomicBool::new(true);
    let result = search_with_cancel(&config, &cancel);
    assert!(
        matches!(result, Err(SearchError::Cancelled)),
        "cancelled search should return SearchError::Cancelled, got {result:?}"
    );
}

#[test]
fn grep_with_cancel_stops_when_cancelled() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    for index in 0..100 {
        let file = temp_path.join(format!("file_{index:03}.txt"));
        let mut handle = File::create(&file).expect("should create file");
        writeln!(handle, "needle").expect("should write file");
    }

    let config = GrepConfig {
        pattern: "needle".to_string(),
        search: SearchConfig {
            paths: vec![PathBuf::from(temp_path)],
            ..Default::default()
        },
        ..Default::default()
    };

    let cancel = AtomicBool::new(true);
    let result = grep_with_cancel(&config, &cancel);
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

    let cancel = AtomicBool::new(false);
    let results = search_with_cancel(&config, &cancel).expect("uncancelled search should succeed");
    assert_eq!(results.len(), 1, "uncancelled search should find the file");
}
