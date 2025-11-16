//! Integration tests for error handling

use fdr_core::{SearchConfig, search};
use std::path::PathBuf;

#[test]
fn search_with_invalid_regex_returns_error() {
    let config = SearchConfig {
        pattern: Some("[invalid(regex".to_string()),
        paths: vec![PathBuf::from(".")],
        ..Default::default()
    };

    let results = search(&config);
    assert!(
        results.is_err(),
        "invalid regex pattern should return error"
    );
}

#[test]
fn search_with_invalid_glob_returns_error() {
    let config = SearchConfig {
        pattern: Some("[invalid".to_string()),
        glob: true,
        paths: vec![PathBuf::from(".")],
        ..Default::default()
    };

    let results = search(&config);
    assert!(results.is_err(), "invalid glob pattern should return error");
}

#[test]
fn search_with_regex_syntax_error() {
    let config = SearchConfig {
        pattern: Some("(?P<invalid)".to_string()),
        paths: vec![PathBuf::from(".")],
        ..Default::default()
    };

    let results = search(&config);
    assert!(results.is_err(), "regex syntax error should return error");
}

#[test]
fn search_with_unclosed_bracket_regex() {
    let config = SearchConfig {
        pattern: Some("[abc".to_string()),
        paths: vec![PathBuf::from(".")],
        ..Default::default()
    };

    let results = search(&config);
    assert!(
        results.is_err(),
        "unclosed bracket regex should return error"
    );
}

#[test]
fn search_with_invalid_exclude_pattern() {
    let config = SearchConfig {
        paths: vec![PathBuf::from(".")],
        exclude: vec!["[invalid".to_string()],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config);
    assert!(
        results.is_err(),
        "invalid exclude pattern should return error"
    );
}

#[test]
fn search_with_very_deep_nesting_pattern() {
    let deep_pattern = "(".repeat(100) + &")".repeat(100);
    let config = SearchConfig {
        pattern: Some(deep_pattern),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config);
    assert!(
        results.is_ok(),
        "should handle deeply nested patterns without panicking"
    );
}

#[test]
fn search_handles_permission_denied_gracefully() {
    let config = SearchConfig {
        paths: vec![PathBuf::from("/root")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config);
    assert!(
        results.is_ok(),
        "should handle permission errors gracefully"
    );
}

#[test]
fn search_with_special_characters_in_pattern() {
    let special_chars = vec![r"\", r".", r"*", r"+", r"?", r"|", r"^", r"$", r"(", r"["];

    for special_char in special_chars {
        let config = SearchConfig {
            pattern: Some(special_char.to_string()),
            paths: vec![PathBuf::from(".")],
            max_depth: Some(1),
            ..Default::default()
        };

        let results = search(&config);
        match results {
            Ok(res) => drop(res),
            Err(_) => {
                assert!(
                    matches!(special_char, "[" | "(" | "*" | "+" | "?" | "\\"),
                    "unexpected error for pattern: {special_char}"
                );
            }
        }
    }
}

#[test]
fn search_with_null_bytes_in_pattern() {
    let config = SearchConfig {
        pattern: Some("test\0null".to_string()),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config);
    assert!(
        results.is_ok(),
        "should handle null bytes in pattern without panicking"
    );
    let results = results.expect("pattern with null bytes should return valid results");
    assert!(
        results.is_empty(),
        "pattern with null bytes should not match any real filenames"
    );
}

#[test]
fn search_with_unicode_pattern() {
    let config = SearchConfig {
        pattern: Some("测试".to_string()),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config).expect("should handle Unicode patterns");
    assert!(
        results.is_empty(),
        "Unicode pattern unlikely to match files in test directory"
    );
}

#[test]
fn search_with_emoji_pattern() {
    let config = SearchConfig {
        pattern: Some("🦀".to_string()),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config).expect("should handle emoji patterns");
    assert!(
        results.is_empty(),
        "emoji pattern unlikely to match files in test directory"
    );
}

#[test]
fn search_with_very_long_pattern() {
    let long_pattern = "a".repeat(10000);
    let config = SearchConfig {
        pattern: Some(long_pattern),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config);
    assert!(results.is_ok(), "should handle very long patterns");
}

#[test]
fn search_with_very_long_extension() {
    let long_ext = "x".repeat(1000);
    let config = SearchConfig {
        extension: Some(long_ext),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config);
    assert!(results.is_ok(), "should handle very long extensions");
}

#[test]
fn search_with_backtracking_regex() {
    let config = SearchConfig {
        pattern: Some("(a+)+b".to_string()),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config);
    assert!(
        results.is_ok(),
        "should handle potentially problematic regex"
    );
}

#[test]
fn search_with_invalid_file_type() {
    let config = SearchConfig {
        file_type: Some("invalid_type".to_string()),
        paths: vec![PathBuf::from(".")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config).expect("should handle invalid file type");
    assert!(
        !results.is_empty(),
        "unknown file type defaults to matching all (returns true for unknown types)"
    );
}

#[test]
fn search_recovers_from_partial_errors() {
    let config = SearchConfig {
        paths: vec![PathBuf::from("."), PathBuf::from("/nonexistent/path/12345")],
        max_depth: Some(1),
        ..Default::default()
    };

    let results = search(&config).expect("should succeed with partial valid paths");
    assert!(
        !results.is_empty(),
        "should still find results from valid paths"
    );
}
