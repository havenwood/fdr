//! Integration tests for search configuration

use fdr_core::{SearchConfig, search};
use std::path::PathBuf;

#[test]
fn search_config_default_values() {
    let config = SearchConfig::default();

    assert!(config.pattern.is_none());
    assert!(config.paths.is_empty());
    assert!(!config.hidden, "hidden should default to false");
    assert!(!config.no_ignore, "no_ignore should default to false");
    assert!(
        !config.case_sensitive,
        "case_sensitive should default to false"
    );
    assert!(!config.glob, "glob should default to false");
    assert!(!config.full_path, "full_path should default to false");
    assert!(config.max_depth.is_none());
    assert!(config.min_depth.is_none());
    assert!(config.file_type.is_none());
    assert!(config.extension.is_none());
    assert!(config.exclude.is_empty());
    assert!(!config.follow, "follow should default to false");
}

#[test]
fn search_with_empty_paths_uses_current_directory() {
    let config = SearchConfig {
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        !results.is_empty(),
        "should default to searching current directory"
    );
}

#[test]
fn search_with_single_path() {
    let config = SearchConfig {
        paths: vec![PathBuf::from(".")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(!results.is_empty(), "should search single path");
}

#[test]
fn search_with_multiple_paths() {
    let config = SearchConfig {
        paths: vec![PathBuf::from("./src"), PathBuf::from("./Cargo.toml")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(!results.is_empty(), "should search multiple paths");
}

#[test]
fn search_nonexistent_path_returns_empty() {
    let config = SearchConfig {
        paths: vec![PathBuf::from("/nonexistent/path/that/does/not/exist/12345")],
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        results.is_empty(),
        "nonexistent path should return empty results"
    );
}

#[test]
fn search_file_path_returns_that_file() {
    let config = SearchConfig {
        paths: vec![PathBuf::from("./Cargo.toml")],
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert_eq!(results.len(), 1, "should return single file");
    assert!(
        results
            .first()
            .is_some_and(|path| path.ends_with("Cargo.toml")),
        "should return the specified file"
    );
}

#[test]
fn search_directory_path_returns_contents() {
    let config = SearchConfig {
        paths: vec![PathBuf::from("./src")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(!results.is_empty(), "should return directory contents");
}

#[test]
fn search_with_relative_path() {
    let config = SearchConfig {
        paths: vec![PathBuf::from("./src")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    for path in &results {
        assert!(
            path.starts_with("./src") || path.starts_with("src"),
            "results should maintain relative path: {path}"
        );
    }
}

#[test]
fn search_max_depth_zero() {
    let config = SearchConfig {
        paths: vec![PathBuf::from(".")],
        max_depth: Some(0),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        results.len() <= 1,
        "max_depth 0 should only return root path"
    );
}

#[test]
fn search_min_depth_greater_than_max_depth() {
    let config = SearchConfig {
        paths: vec![PathBuf::from(".")],
        min_depth: Some(5),
        max_depth: Some(2),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        !results.is_empty(),
        "with max_depth=2, should find results up to that depth"
    );
}

#[test]
fn search_debug_impl_works() {
    let config = SearchConfig {
        pattern: Some("test".to_string()),
        paths: vec![PathBuf::from(".")],
        hidden: true,
        ..Default::default()
    };

    let debug_output = format!("{config:?}");
    assert!(
        debug_output.contains("test"),
        "debug output should contain pattern"
    );
    assert!(
        debug_output.contains("hidden"),
        "debug output should contain field names"
    );
}

#[test]
fn search_allows_all_options_combined() {
    let config = SearchConfig {
        pattern: Some("lib".to_string()),
        paths: vec![PathBuf::from(".")],
        hidden: true,
        no_ignore: false,
        case_sensitive: false,
        glob: false,
        full_path: true,
        max_depth: Some(3),
        min_depth: Some(1),
        file_type: Some("f".to_string()),
        extension: Some("rs".to_string()),
        exclude: vec!["target".to_string()],
        follow: false,
        min_size: None,
        max_size: None,
        changed_within: None,
        changed_before: None,
    };

    let results = search(&config);
    assert!(results.is_ok(), "should handle all options combined");
}

#[test]
fn search_empty_pattern_string_finds_all() {
    let config = SearchConfig {
        pattern: Some(String::new()),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(!results.is_empty(), "empty pattern should match all files");
}

#[test]
fn search_empty_extension_string() {
    let config = SearchConfig {
        extension: Some(String::new()),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config);
    assert!(results.is_ok(), "should handle empty extension");
}

#[test]
fn search_with_dot_in_path() {
    let config = SearchConfig {
        paths: vec![PathBuf::from("././.")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        !results.is_empty(),
        "should handle paths with multiple dots"
    );
}
