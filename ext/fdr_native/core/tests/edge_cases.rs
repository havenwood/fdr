//! Integration tests for edge cases and boundary conditions

use fdr_core::{SearchConfig, search};
use std::fs::{self, File};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn search_empty_directory_returns_empty() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    let empty_subdir = temp_path.join("empty");
    fs::create_dir(&empty_subdir).expect("should create empty dir");

    let config = SearchConfig {
        paths: vec![PathBuf::from(&empty_subdir)],
        file_type: Some("f".to_string()),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        results.is_empty(),
        "should return empty results for empty directory"
    );
}

#[test]
fn search_large_result_set() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    for index in 0..300 {
        let file = temp_path.join(format!("file_{index:04}.txt"));
        File::create(&file).expect("should create file");
    }

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        file_type: Some("f".to_string()),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        results.len() >= 300,
        "should handle large result sets (found {} files)",
        results.len()
    );

    let our_files_count = results
        .iter()
        .filter(|path| {
            use std::path::Path;
            path.contains("file_") && Path::new(path).extension().is_some_and(|ext| ext == "txt")
        })
        .count();
    assert_eq!(our_files_count, 300, "should find all 300 created files");
}

#[test]
fn search_batch_boundary_conditions() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    for &file_count in &[127, 128, 129] {
        let subdir = temp_path.join(format!("test_{file_count}"));
        fs::create_dir(&subdir).expect("should create dir");

        for index in 0..file_count {
            let file = subdir.join(format!("file_{index:03}.txt"));
            File::create(&file).expect("should create file");
        }

        let config = SearchConfig {
            paths: vec![PathBuf::from(&subdir)],
            file_type: Some("f".to_string()),
            ..Default::default()
        };

        let results = search(&config).expect("search should succeed");
        assert_eq!(
            results.len(),
            file_count,
            "should correctly handle {file_count} files (batch boundary)"
        );
    }
}

#[test]
fn search_very_deep_directory_hierarchy() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let mut current_path = temp_dir.path().to_path_buf();

    for level in 0..50 {
        current_path = current_path.join(format!("level_{level}"));
        fs::create_dir(&current_path).expect("should create dir");
    }

    let deep_file = current_path.join("deep_file.txt");
    File::create(&deep_file).expect("should create file");

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_dir.path())],
        pattern: Some("deep_file".to_string()),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        results.iter().any(|path| path.contains("deep_file.txt")),
        "should find files in very deep hierarchies"
    );
}

#[test]
fn search_file_without_extension() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    let no_ext_file = temp_path.join("README");
    File::create(&no_ext_file).expect("should create file");

    let with_ext_file = temp_path.join("README.md");
    File::create(&with_ext_file).expect("should create file");

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        extension: Some("md".to_string()),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");

    assert!(
        results.iter().any(|path| path.contains("README.md")),
        "should find file with extension"
    );
    assert!(
        !results
            .iter()
            .any(|path| path.ends_with("README") && !path.contains("README.md")),
        "should not find file without extension when filtering by extension"
    );
}

#[test]
fn search_multiple_dots_in_filename() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    let multi_dot_file = temp_path.join("file.test.config.json");
    File::create(&multi_dot_file).expect("should create file");

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        extension: Some("json".to_string()),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");

    assert!(
        results
            .iter()
            .any(|path| path.contains("file.test.config.json")),
        "should correctly handle files with multiple dots"
    );
}

#[test]
fn search_hidden_directory_contents() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    let hidden_dir = temp_path.join(".hidden");
    fs::create_dir(&hidden_dir).expect("should create hidden dir");

    let file_in_hidden = hidden_dir.join("file.txt");
    File::create(&file_in_hidden).expect("should create file");

    let config_no_hidden = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        hidden: false,
        pattern: Some("file.txt".to_string()),
        ..Default::default()
    };

    let results_no_hidden = search(&config_no_hidden).expect("search should succeed");
    assert!(
        !results_no_hidden
            .iter()
            .any(|path| path.contains(".hidden")),
        "should not search hidden directories by default"
    );

    let config_with_hidden = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        hidden: true,
        pattern: Some("file.txt".to_string()),
        ..Default::default()
    };

    let results_with_hidden = search(&config_with_hidden).expect("search should succeed");
    assert!(
        results_with_hidden
            .iter()
            .any(|path| path.contains(".hidden/file.txt")),
        "should search hidden directories with hidden flag"
    );
}

#[test]
fn search_files_with_special_characters_in_name() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    let special_names = vec![
        "file with spaces.txt",
        "file-with-dashes.txt",
        "file_with_underscores.txt",
        "file(with)parens.txt",
        "file[with]brackets.txt",
    ];

    for name in &special_names {
        let file = temp_path.join(name);
        File::create(&file).expect("should create file");
    }

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        file_type: Some("f".to_string()),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");

    for name in &special_names {
        assert!(
            results.iter().any(|path| path.contains(name)),
            "should find file with special characters: {name}"
        );
    }
}

#[test]
fn search_unicode_filenames() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    let unicode_file = temp_path.join("文件.txt");
    File::create(&unicode_file).expect("should create Unicode file");

    let emoji_file = temp_path.join("🦀.txt");
    File::create(&emoji_file).expect("should create emoji file");

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        file_type: Some("f".to_string()),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");

    assert!(
        results.iter().any(|path| path.contains("文件.txt")),
        "should find files with Unicode names"
    );
    assert!(
        results.iter().any(|path| path.contains("🦀.txt")),
        "should find files with emoji in names"
    );
}

/// Regression test: Extension filtering must be case-insensitive.
///
/// This test ensures that `extension: Some("txt")` matches files with `.txt`, `.TXT`, `.TxT`, etc.
/// Files are created in separate subdirectories to work correctly on case-insensitive
/// filesystems (like macOS APFS), where `file.txt` and `FILE.TXT` would be the same file.
#[test]
fn search_case_sensitive_extension_filter() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    let lower_dir = temp_path.join("lower");
    fs::create_dir(&lower_dir).expect("should create dir");
    let lowercase = lower_dir.join("file.txt");
    File::create(&lowercase).expect("should create file");

    let upper_dir = temp_path.join("upper");
    fs::create_dir(&upper_dir).expect("should create dir");
    let uppercase = upper_dir.join("FILE.TXT");
    File::create(&uppercase).expect("should create file");

    let mixed_dir = temp_path.join("mixed");
    fs::create_dir(&mixed_dir).expect("should create dir");
    let mixed = mixed_dir.join("File.TxT");
    File::create(&mixed).expect("should create file");

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        extension: Some("txt".to_string()),
        file_type: Some("f".to_string()),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");

    assert!(
        results.iter().any(|path| path.contains("file.txt")),
        "should find lowercase extension"
    );
    assert!(
        results.iter().any(|path| path.contains("FILE.TXT")),
        "should find uppercase extension"
    );
    assert!(
        results.iter().any(|path| path.contains("File.TxT")),
        "should find mixed case extension"
    );
}

#[test]
fn search_combining_all_filters() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    let subdir = temp_path.join("src");
    fs::create_dir(&subdir).expect("should create dir");

    let matching = subdir.join("test_file.rs");
    fs::write(&matching, vec![b'x'; 2048]).expect("should create file");

    let wrong_pattern = subdir.join("other_file.rs");
    fs::write(&wrong_pattern, vec![b'x'; 2048]).expect("should create file");

    let wrong_ext = subdir.join("test_file.txt");
    fs::write(&wrong_ext, vec![b'x'; 2048]).expect("should create file");

    let wrong_size = subdir.join("test_file_small.rs");
    fs::write(&wrong_size, b"x").expect("should create file");

    let config = SearchConfig {
        pattern: Some("test_file".to_string()),
        paths: vec![PathBuf::from(temp_path)],
        extension: Some("rs".to_string()),
        file_type: Some("f".to_string()),
        min_size: Some(1024),
        max_depth: Some(2),
        hidden: false,
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");

    assert!(
        results
            .iter()
            .any(|path| path.contains("test_file.rs") && !path.contains("small")),
        "should find file matching all filters"
    );
    assert!(
        !results.iter().any(|path| path.contains("other_file.rs")),
        "should not find file with wrong pattern"
    );
    assert!(
        !results.iter().any(|path| path.contains("test_file.txt")),
        "should not find file with wrong extension"
    );
    assert!(
        !results
            .iter()
            .any(|path| path.contains("test_file_small.rs")),
        "should not find file with wrong size"
    );
}

#[test]
fn search_empty_pattern_matches_all() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    File::create(temp_path.join("file1.txt")).expect("should create file");
    File::create(temp_path.join("file2.txt")).expect("should create file");

    let config = SearchConfig {
        pattern: Some(String::new()),
        paths: vec![PathBuf::from(temp_path)],
        file_type: Some("f".to_string()),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");

    assert!(results.len() >= 2, "empty pattern should match all files");
}

#[test]
fn search_with_no_paths_uses_current_directory() {
    let config = SearchConfig {
        paths: vec![],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        !results.is_empty(),
        "should search current directory when no paths specified"
    );
}

#[test]
fn search_extremely_long_filename() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    let long_name = "a".repeat(200) + ".txt";
    let long_file = temp_path.join(&long_name);

    if File::create(&long_file).is_ok() {
        let config = SearchConfig {
            paths: vec![PathBuf::from(temp_path)],
            pattern: Some("a".to_string()),
            ..Default::default()
        };

        let results = search(&config).expect("search should succeed");
        assert!(
            results.iter().any(|path| path.contains(&long_name)),
            "should handle extremely long filenames"
        );
    }
}

#[test]
fn search_nested_exclude_patterns() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    let target_dir = temp_path.join("target");
    fs::create_dir(&target_dir).expect("should create dir");

    let nested_target = target_dir.join("nested");
    fs::create_dir(&nested_target).expect("should create dir");

    File::create(nested_target.join("file.txt")).expect("should create file");

    let src_dir = temp_path.join("src");
    fs::create_dir(&src_dir).expect("should create dir");

    File::create(src_dir.join("file.txt")).expect("should create file");

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        exclude: vec!["target".to_string()],
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");

    assert!(
        !results.iter().any(|path| path.contains("/target/")),
        "should exclude entire target directory tree"
    );
    assert!(
        results.iter().any(|path| path.contains("/src/")),
        "should still search non-excluded directories"
    );
}

/// Regression test: The `min_size` filter must correctly exclude empty (0-byte) files.
///
/// This test verifies that:
/// 1. Empty files are found by default (without `min_size` filter)
/// 2. Empty files are excluded when `min_size: Some(1)` is set
/// 3. Non-empty files are still included with the filter
///
/// Note: Filenames use `ends_with()` checks, so they must not overlap as substrings.
/// `"empty.txt"` and `"nonempty.txt"` would fail because `"nonempty.txt".ends_with("empty.txt")` is true!
#[test]
fn search_zero_byte_files() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    let empty_file = temp_path.join("zero_bytes.txt");
    File::create(&empty_file).expect("should create empty file");

    let nonempty_file = temp_path.join("has_content.txt");
    fs::write(&nonempty_file, b"content").expect("should create file");

    let config_all = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        file_type: Some("f".to_string()),
        ..Default::default()
    };

    let results_all = search(&config_all).expect("search should succeed");
    assert!(
        results_all
            .iter()
            .any(|path| path.ends_with("zero_bytes.txt")),
        "should find empty files by default"
    );

    let config_nonempty = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        min_size: Some(1),
        file_type: Some("f".to_string()),
        ..Default::default()
    };

    let results_nonempty = search(&config_nonempty).expect("search should succeed");

    assert!(
        !results_nonempty
            .iter()
            .any(|path| path.ends_with("zero_bytes.txt")),
        "should exclude empty files with min_size filter"
    );
    assert!(
        results_nonempty
            .iter()
            .any(|path| path.ends_with("has_content.txt")),
        "should include non-empty files"
    );
}
