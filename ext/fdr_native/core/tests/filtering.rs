//! Integration tests for file filtering functionality

use fdr_core::{SearchConfig, SearchError, search as search_bytes};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn lossy(path: &[u8]) -> String {
    String::from_utf8_lossy(path).into_owned()
}

fn search(config: &SearchConfig) -> Result<Vec<String>, SearchError> {
    Ok(search_bytes(config)?
        .iter()
        .map(|path| lossy(path))
        .collect())
}

#[test]
fn search_with_extension_filters_correctly() {
    let config = SearchConfig {
        extension: vec!["toml".to_string()],
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
fn search_with_multiple_extensions() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    fs::write(temp_dir.path().join("one.rb"), "").expect("should write rb file");
    fs::write(temp_dir.path().join("two.rs"), "").expect("should write rs file");
    fs::write(temp_dir.path().join("three.txt"), "").expect("should write txt file");

    let results = search(&SearchConfig {
        paths: vec![temp_dir.path().to_path_buf()],
        extension: vec![".rb".to_owned(), "rs".to_owned()],
        ..Default::default()
    })
    .expect("search should succeed");

    assert_eq!(results.len(), 2);
    assert!(results.iter().any(|path| path.ends_with("one.rb")));
    assert!(results.iter().any(|path| path.ends_with("two.rs")));
}

#[test]
fn search_with_file_type_file() {
    let config = SearchConfig {
        file_type: vec!["f".to_string()],
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
        file_type: vec!["d".to_string()],
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
fn search_with_multiple_file_types() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    fs::write(temp_dir.path().join("file"), "").expect("should write file");
    fs::create_dir(temp_dir.path().join("directory")).expect("should create directory");

    let results = search(&SearchConfig {
        paths: vec![temp_dir.path().to_path_buf()],
        file_type: vec!["file".to_owned(), "directory".to_owned()],
        max_depth: Some(1),
        ..Default::default()
    })
    .expect("search should succeed");

    assert_eq!(results.len(), 2);
    assert!(results.iter().any(|path| path.ends_with("file")));
    assert!(results.iter().any(|path| path.ends_with("directory")));
}

#[test]
fn search_with_file_type_aliases() {
    let file_config = SearchConfig {
        file_type: vec!["file".to_string()],
        paths: vec![PathBuf::from(".")],
        max_depth: Some(1),
        ..Default::default()
    };

    let dir_config = SearchConfig {
        file_type: vec!["directory".to_string()],
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

    assert!(!results.is_empty(), "fixture should match something");
    for path in &results {
        let depth = path.matches(std::path::MAIN_SEPARATOR).count();
        assert!(depth >= 2, "path should be at depth >= 2: {path}");
    }
}

#[test]
fn search_min_depth_preserves_hidden_ignore_and_exclude_pruning() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    for directory in [".git", ".hidden", "ignored", "excluded", "visible"] {
        fs::create_dir(temp_path.join(directory)).expect("should create fixture directory");
    }
    fs::write(temp_path.join(".gitignore"), "ignored/\n").expect("should write gitignore");
    for path in [
        ".hidden/secret.txt",
        "ignored/ignored.txt",
        "excluded/excluded.txt",
        "visible/public.txt",
    ] {
        fs::write(temp_path.join(path), "fixture\n").expect("should write fixture");
    }

    let results = search(&SearchConfig {
        paths: vec![temp_path.to_path_buf()],
        min_depth: Some(2),
        file_type: vec!["f".to_string()],
        exclude: vec!["excluded".to_string()],
        ..Default::default()
    })
    .expect("search should succeed");

    assert_eq!(
        results,
        vec![
            temp_path
                .join("visible/public.txt")
                .to_string_lossy()
                .replace('\\', "/")
        ],
        "min_depth must not bypass directory pruning"
    );
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
fn search_combines_extension_and_pattern() {
    let config = SearchConfig {
        pattern: Some("Cargo".to_string()),
        extension: vec!["toml".to_string()],
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
        file_type: vec!["d".to_string()],
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
