//! Integration tests for pattern matching functionality

use fdr_core::{SearchConfig, search};
use std::path::PathBuf;

#[test]
fn search_without_pattern_finds_all_files() {
    let config = SearchConfig {
        paths: vec![PathBuf::from(".")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        !results.is_empty(),
        "should find files in current directory"
    );
}

#[test]
fn search_with_regex_pattern_matches_correctly() {
    let config = SearchConfig {
        pattern: Some("Cargo".to_string()),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(2),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        results.iter().any(|path| path.contains("Cargo")),
        "should find files matching 'Cargo' pattern"
    );
}

#[test]
fn search_with_glob_pattern_matches_files() {
    let config = SearchConfig {
        pattern: Some("*.toml".to_string()),
        glob: true,
        paths: vec![PathBuf::from(".")],
        max_depth: Some(2),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        results.iter().all(|path| {
            std::path::Path::new(path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        }),
        "all results should have .toml extension"
    );
}

#[test]
fn search_case_sensitive_distinguishes_case() {
    let insensitive_config = SearchConfig {
        pattern: Some("cargo".to_string()),
        paths: vec![PathBuf::from(".")],
        case_sensitive: false,
        max_depth: Some(2),
        ..Default::default()
    };

    let sensitive_config = SearchConfig {
        pattern: Some("cargo".to_string()),
        paths: vec![PathBuf::from(".")],
        case_sensitive: true,
        max_depth: Some(2),
        ..Default::default()
    };

    let insensitive_results =
        search(&insensitive_config).expect("insensitive search should succeed");
    let sensitive_results = search(&sensitive_config).expect("sensitive search should succeed");

    assert!(
        insensitive_results
            .iter()
            .any(|path| path.contains("Cargo")),
        "case insensitive search should match 'Cargo'"
    );

    assert!(
        !sensitive_results
            .iter()
            .any(|path| path.contains("Cargo.toml")),
        "case sensitive search should not match 'Cargo.toml' when pattern is 'cargo'"
    );
}

#[test]
fn search_full_path_matches_directory_names() {
    let config = SearchConfig {
        pattern: Some("src".to_string()),
        full_path: true,
        paths: vec![PathBuf::from(".")],
        max_depth: Some(3),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        results.iter().any(|path| path.contains("src")),
        "full path search should match directory names in path"
    );
}

#[test]
fn search_filename_only_ignores_directory_names() {
    let config = SearchConfig {
        pattern: Some("^src$".to_string()),
        full_path: false,
        paths: vec![PathBuf::from(".")],
        max_depth: Some(2),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    for path in &results {
        let filename = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        assert_eq!(filename, "src", "should only match filename 'src'");
    }
}

#[test]
fn search_complex_regex_pattern() {
    let config = SearchConfig {
        pattern: Some(r"^[Cc]argo\.(toml|lock)$".to_string()),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(2),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        results.iter().all(|path| {
            let filename = std::path::Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            filename == "Cargo.toml"
                || filename == "Cargo.lock"
                || filename == "cargo.toml"
                || filename == "cargo.lock"
        }),
        "should only match Cargo.toml or Cargo.lock"
    );
}

#[test]
fn search_glob_with_subdirectory() {
    let config = SearchConfig {
        pattern: Some("src/*.rs".to_string()),
        glob: true,
        full_path: true,
        paths: vec![PathBuf::from(".")],
        max_depth: Some(3),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    for path in &results {
        assert!(
            path.contains("src")
                && std::path::Path::new(path)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("rs")),
            "should match .rs files in src directory: {path}"
        );
    }
}
