//! Integration tests for edge cases and boundary conditions

#[path = "support/search.rs"]
pub mod search_support;

use fdr_core::{SearchConfig, SearchError};
use std::fs::{self, File};
use std::path::PathBuf;
use tempfile::TempDir;

fn lossy(path: &[u8]) -> String {
    String::from_utf8_lossy(path).into_owned()
}

fn search(config: &SearchConfig) -> Result<Vec<String>, SearchError> {
    Ok(search_support::collect(config, false)?
        .iter()
        .map(|path| lossy(path))
        .collect())
}

#[cfg(target_os = "linux")]
#[test]
fn search_includes_non_utf8_filenames() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    File::create(temp_path.join(OsStr::from_bytes(b"bad\xffname.txt")))
        .expect("should create non-UTF-8 filename");

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        ..Default::default()
    };

    let results = search_support::collect(&config, false).expect("search should succeed");
    assert_eq!(results.len(), 1, "non-UTF-8 filename should be emitted");
    let result = results.first().expect("should have one result");
    assert!(
        result.ends_with(b"bad\xffname.txt"),
        "raw bytes should survive, got {result:?}"
    );
}

#[test]
fn search_empty_directory_returns_empty() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    let empty_subdir = temp_path.join("empty");
    fs::create_dir(&empty_subdir).expect("should create empty dir");

    let config = SearchConfig {
        paths: vec![PathBuf::from(&empty_subdir)],
        file_type: vec!["f".to_string()],
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
        file_type: vec!["f".to_string()],
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
        extension: vec!["md".to_string()],
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
        extension: vec!["json".to_string()],
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
        file_type: vec!["f".to_string()],
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
        file_type: vec!["f".to_string()],
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

/// Keeps names in separate directories on case-insensitive filesystems.
#[test]
fn search_case_insensitive_extension_filter() {
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
        extension: vec!["txt".to_string()],
        file_type: vec!["f".to_string()],
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
        extension: vec!["rs".to_string()],
        file_type: vec!["f".to_string()],
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
        file_type: vec!["f".to_string()],
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");

    assert!(results.len() >= 2, "empty pattern should match all files");
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

/// Filenames must not overlap as substrings, since the assertions use `ends_with`.
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
        file_type: vec!["f".to_string()],
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
        file_type: vec!["f".to_string()],
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

#[test]
fn search_bare_dotfile_is_not_its_own_extension() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    File::create(temp_path.join(".rs")).expect("should create file");
    File::create(temp_path.join("main.rs")).expect("should create file");

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        extension: vec!["rs".to_string()],
        hidden: true,
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        results.iter().any(|path| path.ends_with("main.rs")),
        "should match a file that has an extension"
    );
    assert!(
        !results.iter().any(|path| path.ends_with("/.rs")),
        "should not treat a bare dotfile as its own extension"
    );
}

#[test]
fn search_exclude_pattern_anchors_to_the_search_root() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    let vendor = temp_path.join("vendor");
    fs::create_dir(&vendor).expect("should create dir");
    File::create(vendor.join("a.rs")).expect("should create file");
    File::create(vendor.join("keep.rs")).expect("should create file");

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        exclude: vec!["vendor/a.rs".to_string()],
        file_type: vec!["f".to_string()],
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        results.iter().any(|path| path.ends_with("keep.rs")),
        "should keep files the exclude does not name"
    );
    assert!(
        !results.iter().any(|path| path.ends_with("a.rs")),
        "a slash-containing exclude should anchor to the search root"
    );
}

#[test]
#[cfg(unix)]
fn search_sizes_symlinks_by_the_link_not_the_target() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    fs::write(temp_path.join("target.txt"), vec![b'x'; 5000]).expect("should create file");
    std::os::unix::fs::symlink("target.txt", temp_path.join("link.txt"))
        .expect("should create symlink");
    fs::create_dir(temp_path.join("subdir")).expect("should create dir");

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        max_size: Some(100),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        results.iter().any(|path| path.ends_with("link.txt")),
        "should size a symlink by the link itself"
    );
    assert!(
        !results.iter().any(|path| path.ends_with("target.txt")),
        "should exclude a target above max_size"
    );
    assert!(
        !results.iter().any(|path| path.ends_with("subdir")),
        "should apply size filters only to regular files"
    );
}

#[test]
#[cfg(unix)]
fn search_lists_a_root_symlink_whose_target_cannot_be_stat() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    let locked = temp_path.join("locked");

    fs::create_dir(&locked).expect("should create directory");
    fs::write(locked.join("target.txt"), "content").expect("should create file");
    std::os::unix::fs::symlink("locked/target.txt", temp_path.join("unreachable"))
        .expect("should create symlink");
    std::os::unix::fs::symlink("loop_b", temp_path.join("loop_a")).expect("should create symlink");
    std::os::unix::fs::symlink("loop_a", temp_path.join("loop_b")).expect("should create symlink");
    fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o000))
        .expect("should lock directory");

    for name in ["unreachable", "loop_a"] {
        let config = SearchConfig {
            paths: vec![temp_path.join(name)],
            file_type: vec!["l".to_string()],
            ..Default::default()
        };

        let results = search(&config).expect("search should succeed");
        assert_eq!(results.len(), 1, "should list `{name}`: {results:?}");
    }

    fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("should unlock directory");
}

#[test]
#[cfg(unix)]
fn search_lists_broken_symlinks_when_following() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    fs::write(temp_path.join("present.txt"), "content").expect("should create file");
    std::os::unix::fs::symlink("missing_target", temp_path.join("dangling.txt"))
        .expect("should create symlink");

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        follow: true,
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        results.iter().any(|path| path.ends_with("dangling.txt")),
        "follow should still list a broken symlink, like fd"
    );
    assert!(
        results.iter().any(|path| path.ends_with("present.txt")),
        "follow should list regular files"
    );
}

#[test]
#[cfg(unix)]
fn search_does_not_recover_symlink_loops_as_broken_links() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    fs::write(temp_path.join("ok.txt"), "content").expect("should create file");
    std::os::unix::fs::symlink("loop_b", temp_path.join("loop_a")).expect("should create symlink");
    std::os::unix::fs::symlink("loop_a", temp_path.join("loop_b")).expect("should create symlink");

    let mut config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        follow: true,
        ..Default::default()
    };

    let results = search(&config).expect("search should skip symlink loops");
    assert_eq!(results.len(), 1);
    assert!(results.first().is_some_and(|path| path.ends_with("ok.txt")));

    config.raise_on_error = true;
    assert!(search(&config).is_err());
}

#[test]
#[cfg(unix)]
fn search_type_symlink_with_follow_matches_only_broken_links() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    fs::write(temp_path.join("present.txt"), "content").expect("should create file");
    fs::create_dir(temp_path.join("subdir")).expect("should create dir");
    std::os::unix::fs::symlink("present.txt", temp_path.join("good_file_link"))
        .expect("should create symlink");
    std::os::unix::fs::symlink("subdir", temp_path.join("good_dir_link"))
        .expect("should create symlink");
    std::os::unix::fs::symlink("missing_target", temp_path.join("dangling.txt"))
        .expect("should create symlink");

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        file_type: vec!["l".to_string()],
        follow: true,
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert_eq!(
        results.len(),
        1,
        "followed good symlinks take their target's type, like fd -L -t l"
    );
    assert!(
        results.iter().any(|path| path.ends_with("dangling.txt")),
        "a broken symlink cannot be followed, so it stays a symlink"
    );
}

#[test]
#[cfg(unix)]
fn search_min_depth_applies_to_broken_symlinks() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    fs::create_dir(temp_path.join("subdir")).expect("should create dir");
    std::os::unix::fs::symlink("missing_target", temp_path.join("shallow_link"))
        .expect("should create symlink");
    std::os::unix::fs::symlink("missing_target", temp_path.join("subdir/deep_link"))
        .expect("should create symlink");

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        follow: true,
        min_depth: Some(2),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        results.iter().any(|path| path.ends_with("deep_link")),
        "should keep a broken symlink at min_depth"
    );
    assert!(
        !results.iter().any(|path| path.ends_with("shallow_link")),
        "min_depth should drop a shallow broken symlink, unlike fd's drop-all"
    );
}

#[test]
#[cfg(unix)]
fn search_sizes_exclude_broken_symlinks_when_following() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();

    fs::write(temp_path.join("present.txt"), "content").expect("should create file");
    std::os::unix::fs::symlink("missing_target", temp_path.join("dangling.txt"))
        .expect("should create symlink");

    let config = SearchConfig {
        paths: vec![PathBuf::from(temp_path)],
        follow: true,
        min_size: Some(1),
        ..Default::default()
    };

    let results = search(&config).expect("search should succeed");
    assert!(
        !results.iter().any(|path| path.ends_with("dangling.txt")),
        "size filters apply only to regular files, like fd -L -S"
    );
}
