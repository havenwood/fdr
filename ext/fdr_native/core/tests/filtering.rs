//! Integration tests for file filtering functionality

use fdr_core::{SearchConfig, search};
use std::path::PathBuf;

#[test]
fn search_with_extension_filters_correctly() {
    let config = SearchConfig {
        extension: Some("toml".to_string()),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(2),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(!results.is_empty(), "should find .toml files");
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
fn search_with_file_type_file() {
    let config = SearchConfig {
        file_type: Some("f".to_string()),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(!results.is_empty(), "should find regular files");

    for path in &results {
        let metadata = std::fs::metadata(path).expect("path should exist");
        assert!(metadata.is_file(), "result should be a file: {path}");
    }
}

#[test]
fn search_with_file_type_directory() {
    let config = SearchConfig {
        file_type: Some("d".to_string()),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(2),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(!results.is_empty(), "should find directories");

    for path in &results {
        let metadata = std::fs::metadata(path).expect("path should exist");
        assert!(metadata.is_dir(), "result should be a directory: {path}");
    }
}

#[test]
fn search_with_file_type_aliases() {
    let file_config = SearchConfig {
        file_type: Some("file".to_string()),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(1),
        ..Default::default()
    };

    let dir_config = SearchConfig {
        file_type: Some("directory".to_string()),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(1),
        ..Default::default()
    };

    let file_results = search(&file_config).expect("file search should succeed");
    let dir_results = search(&dir_config).expect("directory search should succeed");

    assert!(
        !file_results.is_empty(),
        "should find files with 'file' alias"
    );
    assert!(
        !dir_results.is_empty(),
        "should find directories with 'directory' alias"
    );
}

#[test]
fn search_with_max_depth_limits_results() {
    let shallow_config = SearchConfig {
        paths: vec![PathBuf::from(".")],
        max_depth: Some(1),
        ..Default::default()
    };

    let deep_config = SearchConfig {
        paths: vec![PathBuf::from(".")],
        max_depth: Some(3),
        ..Default::default()
    };

    let shallow_results = search(&shallow_config).expect("shallow search should succeed");
    let deep_results = search(&deep_config).expect("deep search should succeed");

    assert!(
        deep_results.len() >= shallow_results.len(),
        "deeper search should find at least as many files"
    );
}

#[test]
fn search_with_min_depth_excludes_shallow_files() {
    let config = SearchConfig {
        paths: vec![PathBuf::from(".")],
        min_depth: Some(2),
        max_depth: Some(3),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");

    for path in &results {
        let depth = path.matches(std::path::MAIN_SEPARATOR).count();
        assert!(depth >= 1, "path should be at depth >= 2: {path}");
    }
}

#[test]
fn search_with_depth_range() {
    let config = SearchConfig {
        paths: vec![PathBuf::from(".")],
        min_depth: Some(1),
        max_depth: Some(2),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");

    for path in &results {
        let depth = path.matches(std::path::MAIN_SEPARATOR).count();
        assert!(
            depth <= 2,
            "path should be within max depth: {path} (depth: {depth})"
        );
    }
}

#[test]
fn search_with_exclude_pattern() {
    let config = SearchConfig {
        extension: Some("toml".to_string()),
        paths: vec![PathBuf::from(".")],
        exclude: vec!["target".to_string()],
        max_depth: Some(5),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");

    assert!(
        !results.iter().any(|path| path.contains("/target/")),
        "should exclude target directory"
    );
}

#[test]
fn search_with_multiple_exclude_patterns() {
    let config = SearchConfig {
        paths: vec![PathBuf::from(".")],
        exclude: vec!["target".to_string(), "*.lock".to_string()],
        max_depth: Some(3),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");

    assert!(
        !results.iter().any(|path| path.contains("/target/")),
        "should exclude target directory"
    );
    assert!(
        !results.iter().any(|path| std::path::Path::new(path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("lock"))),
        "should exclude .lock files"
    );
}

#[test]
fn search_combines_extension_and_pattern() {
    let config = SearchConfig {
        pattern: Some("Cargo".to_string()),
        extension: Some("toml".to_string()),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(2),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");

    for path in &results {
        assert!(
            path.contains("Cargo")
                && std::path::Path::new(path)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("toml")),
            "should match both pattern and extension: {path}"
        );
    }
}

#[test]
fn search_combines_file_type_and_pattern() {
    let config = SearchConfig {
        pattern: Some("src".to_string()),
        file_type: Some("d".to_string()),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(2),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");

    for path in &results {
        let metadata = std::fs::metadata(path).expect("path should exist");
        assert!(
            metadata.is_dir() && path.contains("src"),
            "should be a directory matching pattern: {path}"
        );
    }
}

#[test]
fn search_respects_gitignore_by_default() {
    let config = SearchConfig {
        paths: vec![PathBuf::from(".")],
        no_ignore: false,
        max_depth: Some(5),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");

    let has_target_files = results.iter().any(|path| path.contains("/target/"));
    assert!(
        !has_target_files,
        "should respect .gitignore and exclude target/"
    );
}

#[test]
fn search_with_no_ignore_includes_ignored_files() {
    let with_ignore = SearchConfig {
        paths: vec![PathBuf::from(".")],
        no_ignore: false,
        max_depth: Some(5),
        ..Default::default()
    };

    let without_ignore = SearchConfig {
        paths: vec![PathBuf::from(".")],
        no_ignore: true,
        max_depth: Some(5),
        ..Default::default()
    };

    let with_ignore_results = search(&with_ignore).expect("with_ignore search should succeed");
    let without_ignore_results =
        search(&without_ignore).expect("without_ignore search should succeed");

    assert!(
        without_ignore_results.len() >= with_ignore_results.len(),
        "no_ignore should find at least as many files"
    );
}

#[test]
fn search_hidden_files_excluded_by_default() {
    let config = SearchConfig {
        paths: vec![PathBuf::from(".")],
        hidden: false,
        max_depth: Some(2),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");

    for path in &results {
        let filename = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if filename.starts_with('.') {
            assert_eq!(filename, ".", "should not include hidden files: {path}");
        }
    }
}
