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
fn search_glob_star_does_not_cross_a_path_separator() {
    let temp_dir = tempfile::TempDir::new().expect("should create temp dir");
    let direct = temp_dir.path().join("file.rs");
    let nested_dir = temp_dir.path().join("src");
    std::fs::create_dir(&nested_dir).expect("should create nested directory");
    std::fs::write(&direct, "").expect("should write direct fixture");
    std::fs::write(nested_dir.join("file.rs"), "").expect("should write nested fixture");

    let config = SearchConfig {
        pattern: Some(temp_dir.path().join("*.rs").to_string_lossy().into_owned()),
        glob: true,
        full_path: true,
        paths: vec![temp_dir.path().to_path_buf()],
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert_eq!(results, vec![direct.to_string_lossy().into_owned()]);
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
        pattern: Some("^/.*/src$".to_string()),
        full_path: true,
        paths: vec![PathBuf::from(".")],
        max_depth: Some(3),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        results.iter().any(|path| path.ends_with("src")),
        "full path search should match directory names in the absolute path"
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
        pattern: Some("**/src/*.rs".to_string()),
        glob: true,
        full_path: true,
        paths: vec![PathBuf::from(".")],
        max_depth: Some(3),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(!results.is_empty(), "should match .rs files under src");
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

#[test]
fn search_glob_full_path_anchors_to_the_absolute_path() {
    // An anchored relative glob cannot match an absolute full path, like fd.
    let config = SearchConfig {
        pattern: Some("src/*.rs".to_string()),
        glob: true,
        full_path: true,
        paths: vec![PathBuf::from(".")],
        max_depth: Some(3),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        results.is_empty(),
        "a glob without a leading wildcard should not match subpaths"
    );
}
