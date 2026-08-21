//! Integration tests for file content search

use fdr_core::{GrepConfig, SearchConfig, grep};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn search_under(path: &Path) -> SearchConfig {
    SearchConfig {
        paths: vec![PathBuf::from(path)],
        ..Default::default()
    }
}

fn needle_in(search: SearchConfig) -> GrepConfig {
    GrepConfig {
        pattern: "needle".to_string(),
        search,
        ..Default::default()
    }
}

#[test]
fn grep_defaults_to_case_sensitive_content_and_case_insensitive_names() {
    let config = GrepConfig::default();

    assert!(config.content_case_sensitive);
    assert!(!config.search.case_sensitive);
}

#[test]
fn grep_groups_one_based_matching_lines_by_file() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let path = temp_dir.path().join("example.rb");
    fs::write(&path, "first\nneedle\nthird\nneedle twice needle\n").expect("should write fixture");

    let results = grep(&needle_in(search_under(temp_dir.path()))).expect("grep should succeed");

    assert_eq!(results.len(), 1, "should find one file");
    let result = results.first().expect("should find one file");
    assert_eq!(result.path, path.to_string_lossy());
    assert_eq!(
        result.lines,
        vec![
            (2, b"needle".to_vec()),
            (4, b"needle twice needle".to_vec())
        ],
        "a line matching twice should be reported once"
    );
}

#[test]
fn grep_honors_content_case_sensitivity() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    fs::write(temp_dir.path().join("case.txt"), "Needle\nneedle\n").expect("should write fixture");

    let sensitive = grep(&needle_in(search_under(temp_dir.path())))
        .expect("case-sensitive grep should succeed");
    let insensitive = grep(&GrepConfig {
        content_case_sensitive: false,
        ..needle_in(search_under(temp_dir.path()))
    })
    .expect("case-insensitive grep should succeed");

    let sensitive_result = sensitive.first().expect("should find sensitive match");
    let insensitive_result = insensitive
        .first()
        .expect("should find insensitive matches");
    assert_eq!(sensitive_result.lines, vec![(2, b"needle".to_vec())]);
    assert_eq!(
        insensitive_result.lines,
        vec![(1, b"Needle".to_vec()), (2, b"needle".to_vec())]
    );
}

#[test]
fn grep_respects_gitignore_by_default() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    fs::create_dir(temp_path.join(".git")).expect("should create .git directory");
    fs::write(temp_path.join(".gitignore"), "ignored.txt\n").expect("should write gitignore");
    fs::write(temp_path.join("visible.txt"), "needle\n").expect("should write fixture");
    fs::write(temp_path.join("ignored.txt"), "needle\n").expect("should write fixture");

    let default_results = grep(&needle_in(search_under(temp_path))).expect("grep should succeed");
    let all_results = grep(&needle_in(SearchConfig {
        no_ignore: true,
        ..search_under(temp_path)
    }))
    .expect("grep without ignore rules should succeed");

    assert_eq!(default_results.len(), 1);
    assert!(
        default_results
            .first()
            .is_some_and(|result| result.path.ends_with("visible.txt")),
        "should skip the ignored file"
    );
    assert_eq!(all_results.len(), 2, "no_ignore should include both files");
}

#[test]
fn grep_skips_hidden_files_by_default() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    fs::write(temp_path.join(".hidden.txt"), "needle\n").expect("should write fixture");
    fs::write(temp_path.join("plain.txt"), "needle\n").expect("should write fixture");

    let default_results = grep(&needle_in(search_under(temp_path))).expect("grep should succeed");
    let with_hidden = grep(&needle_in(SearchConfig {
        hidden: true,
        ..search_under(temp_path)
    }))
    .expect("grep including hidden files should succeed");

    assert_eq!(default_results.len(), 1, "should skip the hidden file");
    assert_eq!(with_hidden.len(), 2, "hidden should include both files");
}

#[test]
fn grep_skips_excluded_patterns() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    fs::create_dir(temp_path.join("vendor")).expect("should create vendor directory");
    fs::write(temp_path.join("keep.txt"), "needle\n").expect("should write fixture");
    fs::write(temp_path.join("vendor/skip.txt"), "needle\n").expect("should write fixture");

    let results = grep(&needle_in(SearchConfig {
        exclude: vec!["vendor".to_string()],
        ..search_under(temp_path)
    }))
    .expect("grep should succeed");

    assert_eq!(results.len(), 1);
    assert!(
        results
            .first()
            .is_some_and(|result| result.path.ends_with("keep.txt")),
        "should skip the excluded directory"
    );
}

#[test]
fn grep_min_depth_preserves_hidden_ignore_and_exclude_pruning() {
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
        fs::write(temp_path.join(path), "needle\n").expect("should write fixture");
    }

    let results = grep(&needle_in(SearchConfig {
        min_depth: Some(2),
        exclude: vec!["excluded".to_string()],
        ..search_under(temp_path)
    }))
    .expect("grep should succeed");

    assert_eq!(results.len(), 1);
    assert!(
        results
            .first()
            .is_some_and(|result| result.path.ends_with("visible/public.txt")),
        "min_depth must not bypass directory pruning"
    );
}

#[test]
fn grep_searches_multiple_paths() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    fs::create_dir(temp_path.join("lib")).expect("should create lib directory");
    fs::create_dir(temp_path.join("spec")).expect("should create spec directory");
    fs::create_dir(temp_path.join("doc")).expect("should create doc directory");
    fs::write(temp_path.join("lib/a.txt"), "needle\n").expect("should write fixture");
    fs::write(temp_path.join("spec/b.txt"), "needle\n").expect("should write fixture");
    fs::write(temp_path.join("doc/c.txt"), "needle\n").expect("should write fixture");

    let results = grep(&needle_in(SearchConfig {
        paths: vec![temp_path.join("lib"), temp_path.join("spec")],
        ..Default::default()
    }))
    .expect("grep should succeed");

    assert_eq!(results.len(), 2, "should search both given paths");
    assert!(
        results.iter().all(|result| !result.path.contains("/doc/")),
        "should not search paths that were not given"
    );
}

#[test]
fn grep_filters_by_extension() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    fs::write(temp_path.join("code.rb"), "needle\n").expect("should write fixture");
    fs::write(temp_path.join("notes.txt"), "needle\n").expect("should write fixture");

    let results = grep(&needle_in(SearchConfig {
        extension: vec!["rb".to_string()],
        ..search_under(temp_path)
    }))
    .expect("grep should succeed");

    assert_eq!(results.len(), 1);
    assert!(
        results
            .first()
            .is_some_and(|result| result.path.ends_with("code.rb")),
        "should only read files with the given extension"
    );
}

#[test]
fn grep_filters_by_filename_pattern() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    fs::write(temp_path.join("alpha_spec.rb"), "needle\n").expect("should write fixture");
    fs::write(temp_path.join("alpha.rb"), "needle\n").expect("should write fixture");

    let results = grep(&needle_in(SearchConfig {
        pattern: Some("_spec\\.rb$".to_string()),
        case_sensitive: true,
        ..search_under(temp_path)
    }))
    .expect("grep should succeed");

    assert_eq!(results.len(), 1);
    assert!(
        results
            .first()
            .is_some_and(|result| result.path.ends_with("alpha_spec.rb")),
        "should only read files matching the filename pattern"
    );
}

#[test]
fn grep_respects_depth_limits() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    fs::create_dir(temp_path.join("nested")).expect("should create nested directory");
    fs::write(temp_path.join("shallow.txt"), "needle\n").expect("should write fixture");
    fs::write(temp_path.join("nested/deep.txt"), "needle\n").expect("should write fixture");

    let results = grep(&needle_in(SearchConfig {
        max_depth: Some(1),
        ..search_under(temp_path)
    }))
    .expect("grep should succeed");

    assert_eq!(results.len(), 1);
    assert!(
        results
            .first()
            .is_some_and(|result| result.path.ends_with("shallow.txt")),
        "should not descend past max_depth"
    );
}

#[test]
fn grep_filters_by_size() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    fs::write(temp_path.join("small.txt"), "needle\n").expect("should write fixture");
    fs::write(temp_path.join("large.txt"), "needle\n".repeat(100)).expect("should write fixture");

    let results = grep(&needle_in(SearchConfig {
        min_size: Some(100),
        ..search_under(temp_path)
    }))
    .expect("grep should succeed");

    assert_eq!(results.len(), 1);
    assert!(
        results
            .first()
            .is_some_and(|result| result.path.ends_with("large.txt")),
        "should skip files below min_size"
    );
}

#[test]
#[cfg(unix)]
fn grep_follows_symlinks_when_enabled() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    let target = temp_path.join("target.txt");
    let root = temp_path.join("root");
    fs::create_dir(&root).expect("should create root directory");
    fs::write(&target, "needle\n").expect("should write fixture");
    std::os::unix::fs::symlink(&target, root.join("link.txt")).expect("should create symlink");

    let default_results = grep(&needle_in(search_under(&root))).expect("grep should succeed");
    let followed = grep(&needle_in(SearchConfig {
        follow: true,
        ..search_under(&root)
    }))
    .expect("grep following symlinks should succeed");

    assert!(default_results.is_empty(), "should not read symlinks");
    assert_eq!(followed.len(), 1, "follow should read through the symlink");
}

#[test]
fn grep_skips_binary_files() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    fs::write(temp_dir.path().join("binary.bin"), b"needle\n\0needle\n")
        .expect("should write binary fixture");

    let results = grep(&needle_in(search_under(temp_dir.path()))).expect("grep should succeed");

    assert!(results.is_empty(), "should skip binary files entirely");
}

#[test]
fn grep_returns_empty_when_nothing_matches() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    fs::write(temp_dir.path().join("plain.txt"), "haystack\n").expect("should write fixture");

    let results = grep(&needle_in(search_under(temp_dir.path()))).expect("grep should succeed");

    assert!(
        results.is_empty(),
        "should return no results without a match"
    );
}

#[test]
fn grep_returns_results_sorted_by_path() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    for index in 0..200 {
        let dir = temp_path.join(format!("dir_{index:04}"));
        fs::create_dir(&dir).expect("should create directory");
        fs::write(dir.join("file.txt"), "needle\n").expect("should write fixture");
    }

    let results = grep(&needle_in(search_under(temp_path))).expect("grep should succeed");
    let paths: Vec<&str> = results.iter().map(|result| result.path.as_str()).collect();
    let mut sorted = paths.clone();
    sorted.sort_unstable();

    assert_eq!(paths.len(), 200);
    assert_eq!(paths, sorted, "results should be ordered by path");
}

#[test]
fn grep_matches_across_the_serial_byte_limit() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    for index in 0..5 {
        let path = temp_path.join(format!("file_{index}.txt"));
        let contents = format!("{}needle\n", "x\n".repeat(1_050_000));
        fs::write(&path, contents).expect("should write fixture");
    }

    let results = grep(&needle_in(search_under(temp_path))).expect("grep should succeed");
    let paths: Vec<&str> = results.iter().map(|result| result.path.as_str()).collect();
    let mut deduplicated = paths.clone();
    deduplicated.dedup();

    assert_eq!(
        results.len(),
        5,
        "bailing to the parallel walker should not drop results"
    );
    assert_eq!(paths, deduplicated, "no path should be reported twice");
}

#[test]
fn grep_rejects_invalid_patterns() {
    let result = grep(&GrepConfig {
        pattern: "[invalid".to_string(),
        search: SearchConfig {
            paths: vec![PathBuf::from(".")],
            ..Default::default()
        },
        ..Default::default()
    });

    assert!(result.is_err(), "invalid pattern should return an error");
}

#[test]
fn grep_rejects_patterns_matching_a_line_terminator() {
    let result = grep(&GrepConfig {
        pattern: "foo\nbar".to_string(),
        search: SearchConfig {
            paths: vec![PathBuf::from(".")],
            ..Default::default()
        },
        ..Default::default()
    });

    assert!(
        result.is_err(),
        "a pattern spanning lines should return an error"
    );
}

#[test]
fn grep_returns_matching_line_text_without_its_terminator() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    fs::write(
        temp_path.join("a.txt"),
        "alpha\nneedle here\nbeta\nanother needle\n",
    )
    .expect("should write fixture");

    let results = grep(&needle_in(search_under(temp_path))).expect("grep should succeed");

    let result = results.first().expect("should match one file");
    assert_eq!(
        result.lines,
        vec![
            (2, b"needle here".to_vec()),
            (4, b"another needle".to_vec())
        ]
    );
}

#[test]
fn grep_reports_a_line_with_two_matches_once() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    fs::write(temp_path.join("a.txt"), "needle and needle again\n").expect("should write fixture");

    let results = grep(&needle_in(search_under(temp_path))).expect("grep should succeed");

    let result = results.first().expect("should match one file");
    assert_eq!(result.lines, vec![(1, b"needle and needle again".to_vec())]);
}

#[test]
fn grep_trims_a_carriage_return_and_handles_a_missing_final_newline() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    fs::write(temp_path.join("a.txt"), "needle one\r\nneedle two").expect("should write fixture");

    let results = grep(&needle_in(search_under(temp_path))).expect("grep should succeed");

    let result = results.first().expect("should match one file");
    assert_eq!(
        result.lines,
        vec![(1, b"needle one".to_vec()), (2, b"needle two".to_vec())]
    );
}

#[test]
fn grep_matches_anchors_before_lf_and_crlf_terminators() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    fs::write(
        temp_path.join("a.txt"),
        "needle\nother\nneedle\r\nother\r\n",
    )
    .expect("should write fixture");

    let results = grep(&GrepConfig {
        pattern: "^needle$".to_string(),
        search: search_under(temp_path),
        ..Default::default()
    })
    .expect("grep should succeed");

    let result = results.first().expect("should match one file");
    assert_eq!(
        result.lines,
        vec![(1, b"needle".to_vec()), (3, b"needle".to_vec())]
    );
}

#[test]
fn grep_keeps_a_trailing_carriage_return_that_is_not_a_terminator() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    fs::write(temp_path.join("a.txt"), "one needle\r\ntwo needle\r").expect("should write fixture");

    let results = grep(&needle_in(search_under(temp_path))).expect("grep should succeed");

    let result = results.first().expect("should match one file");
    assert_eq!(
        result.lines,
        vec![(1, b"one needle".to_vec()), (2, b"two needle\r".to_vec())]
    );
}

#[test]
fn grep_keeps_a_byte_order_mark_in_the_line_text() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    fs::write(temp_path.join("a.txt"), "\u{feff}needle here\n").expect("should write fixture");

    let results = grep(&needle_in(search_under(temp_path))).expect("grep should succeed");

    let result = results.first().expect("should match one file");
    assert_eq!(
        result.lines,
        vec![(1, "\u{feff}needle here".as_bytes().to_vec())]
    );
}

#[test]
#[cfg(unix)]
fn grep_skips_broken_symlinks_when_following() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let temp_path = temp_dir.path();
    fs::write(temp_path.join("real.txt"), "needle\n").expect("should write fixture");
    std::os::unix::fs::symlink("missing_target", temp_path.join("dangling.txt"))
        .expect("should create symlink");

    let results = grep(&needle_in(SearchConfig {
        follow: true,
        ..search_under(temp_path)
    }))
    .expect("grep should skip broken symlinks");

    assert_eq!(results.len(), 1, "should match only the readable file");
    assert!(
        results
             .first()
             .is_some_and(|result| result.path.ends_with("real.txt")),
        "the match should come from the real file"
    );
}
