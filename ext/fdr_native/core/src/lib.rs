//! File search library in the style of `fd`
#[cfg(all(
    not(windows),
    not(target_os = "android"),
    not(target_os = "macos"),
    not(target_os = "freebsd"),
    not(target_os = "openbsd"),
    not(target_os = "illumos"),
    not(all(target_env = "musl", target_pointer_width = "32")),
    not(target_arch = "riscv64")
))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent configuration options with no logical relationship"
)]
pub struct SearchConfig {
    pub pattern: Option<String>,
    pub paths: Vec<PathBuf>,
    pub hidden: bool,
    pub no_ignore: bool,
    pub case_sensitive: bool,
    pub glob: bool,
    pub full_path: bool,
    pub max_depth: Option<usize>,
    pub min_depth: Option<usize>,
    pub file_type: Option<String>,
    pub extension: Option<String>,
    pub exclude: Vec<String>,
    pub follow: bool,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub changed_within: Option<i64>,
    pub changed_before: Option<i64>,
}

fn build_pattern_regex(config: &SearchConfig) -> Result<Option<regex::bytes::Regex>> {
    use regex::bytes::RegexBuilder;

    if let Some(ref pat) = config.pattern {
        let regex_pattern = if config.glob {
            glob_to_regex(pat)?
        } else {
            pat.clone()
        };

        Ok(Some(
            RegexBuilder::new(&regex_pattern)
                .case_insensitive(!config.case_sensitive)
                .build()?,
        ))
    } else {
        Ok(None)
    }
}

fn build_extension_regex(config: &SearchConfig) -> Result<Option<regex::bytes::Regex>> {
    use regex::bytes::RegexBuilder;

    if let Some(ref ext) = config.extension {
        let pattern = format!(r"\.{}$", regex::escape(ext));
        Ok(Some(
            RegexBuilder::new(&pattern).case_insensitive(true).build()?,
        ))
    } else {
        Ok(None)
    }
}

fn configure_walker(builder: &mut ignore::WalkBuilder, config: &SearchConfig) -> Result<()> {
    builder
        .hidden(!config.hidden)
        .ignore(!config.no_ignore)
        .git_ignore(!config.no_ignore)
        .follow_links(config.follow)
        .max_depth(config.max_depth)
        .min_depth(config.min_depth);

    if !config.exclude.is_empty() {
        let mut overrides = ignore::overrides::OverrideBuilder::new(".");
        for pattern in &config.exclude {
            overrides.add(&format!("!{pattern}"))?;
        }
        builder.overrides(overrides.build()?);
    }

    Ok(())
}

/// Batch size for result collection (same as fd's default).
const BATCH_SIZE: usize = 256;

/// Wrapper for batched result sending with automatic flush on drop.
struct ResultBatch {
    batch: Vec<String>,
    sender: crossbeam_channel::Sender<Vec<String>>,
}

impl ResultBatch {
    fn new(sender: crossbeam_channel::Sender<Vec<String>>) -> Self {
        Self {
            batch: Vec::with_capacity(BATCH_SIZE),
            sender,
        }
    }

    fn push(&mut self, item: String) {
        self.batch.push(item);
        if self.batch.len() >= BATCH_SIZE {
            self.flush();
        }
    }

    fn flush(&mut self) {
        if !self.batch.is_empty() {
            let batch = std::mem::replace(&mut self.batch, Vec::with_capacity(BATCH_SIZE));
            drop(self.sender.send(batch));
        }
    }
}

impl Drop for ResultBatch {
    fn drop(&mut self) {
        self.flush();
    }
}

fn matches_metadata_filters(
    entry: &ignore::DirEntry,
    min_size: Option<u64>,
    max_size: Option<u64>,
    changed_within: Option<i64>,
    changed_before: Option<i64>,
) -> bool {
    if min_size.is_none()
        && max_size.is_none()
        && changed_within.is_none()
        && changed_before.is_none()
    {
        return true;
    }

    let Ok(metadata) = entry.metadata() else {
        return false;
    };

    if let Some(min) = min_size
        && metadata.len() < min
    {
        return false;
    }

    if let Some(max) = max_size
        && metadata.len() > max
    {
        return false;
    }
    if (changed_within.is_some() || changed_before.is_some())
        && let Ok(modified) = metadata.modified()
        && let Ok(duration_since_epoch) = modified.duration_since(std::time::UNIX_EPOCH)
    {
        let file_time = i64::try_from(duration_since_epoch.as_secs()).unwrap_or(i64::MAX);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
            .unwrap_or(0);

        if let Some(within_seconds) = changed_within {
            let cutoff = now.saturating_sub(within_seconds);
            if file_time < cutoff {
                return false;
            }
        }

        if let Some(before_seconds) = changed_before {
            let cutoff = now.saturating_sub(before_seconds);
            if file_time > cutoff {
                return false;
            }
        }
    }

    true
}

pub fn search(config: &SearchConfig) -> Result<Vec<String>> {
    use crossbeam_channel::unbounded;
    use ignore::{WalkBuilder, WalkState};
    use std::sync::Arc;

    let pattern = build_pattern_regex(config)?;
    let extension = build_extension_regex(config)?;

    let search_paths: Vec<PathBuf> = if config.paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        config.paths.clone()
    };

    let (first_path, rest) = search_paths
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("No paths to search"))?;
    let mut builder = WalkBuilder::new(first_path);

    for path in rest {
        builder.add(path);
    }

    configure_walker(&mut builder, config)?;

    let pattern = Arc::new(pattern);
    let extension = Arc::new(extension);
    let full_path = config.full_path;
    let file_type = Arc::new(config.file_type.clone());
    let min_size = config.min_size;
    let max_size = config.max_size;
    let changed_within = config.changed_within;
    let changed_before = config.changed_before;

    let (tx, rx) = unbounded();

    let walker = builder.build_parallel();

    walker.run(|| {
        let tx = tx.clone();
        let pattern = Arc::clone(&pattern);
        let extension = Arc::clone(&extension);
        let file_type = Arc::clone(&file_type);

        let mut batch = ResultBatch::new(tx);

        Box::new(move |entry| {
            let Ok(entry) = entry else {
                return WalkState::Continue;
            };

            if entry.depth() == 0 && entry.file_type().is_some_and(|t| t.is_dir()) {
                return WalkState::Continue;
            }

            let path = entry.path();

            let search_str = if full_path {
                path.to_string_lossy()
            } else {
                path.file_name().unwrap_or_default().to_string_lossy()
            };

            if let Some(regex) = pattern.as_ref()
                && !regex.is_match(search_str.as_bytes())
            {
                return WalkState::Continue;
            }

            if let Some(ext_regex) = extension.as_ref()
                && !ext_regex.is_match(search_str.as_bytes())
            {
                return WalkState::Continue;
            }

            if let Some(ref ft) = *file_type {
                let entry_file_type = entry.file_type();
                let matches = match ft.as_str() {
                    "f" | "file" => entry_file_type.is_some_and(|t| t.is_file()),
                    "d" | "dir" | "directory" => entry_file_type.is_some_and(|t| t.is_dir()),
                    "l" | "symlink" => entry_file_type.is_some_and(|t| t.is_symlink()),
                    _ => true,
                };

                if !matches {
                    return WalkState::Continue;
                }
            }

            if !matches_metadata_filters(&entry, min_size, max_size, changed_within, changed_before)
            {
                return WalkState::Continue;
            }

            if let Some(path_str) = path.to_str() {
                batch.push(path_str.to_string());
            }

            WalkState::Continue
        })
    });

    drop(tx);
    let batches: Vec<Vec<String>> = rx.iter().collect();
    let total_size: usize = batches.iter().map(Vec::len).sum();
    let mut results = Vec::with_capacity(total_size);

    for batch in batches {
        results.extend(batch);
    }

    Ok(results)
}

fn glob_to_regex(glob: &str) -> Result<String> {
    use globset::GlobBuilder;

    let glob_pattern = GlobBuilder::new(glob).literal_separator(true).build()?;

    Ok(glob_pattern.regex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_to_regex_converts_simple_glob() {
        let result = glob_to_regex("*.rs").expect("should convert *.rs glob");
        let regex = regex::bytes::Regex::new(&result).expect("should compile to valid regex");
        assert!(regex.is_match(b"file.rs"));
        assert!(!regex.is_match(b"file.toml"));
    }

    #[test]
    fn glob_to_regex_converts_complex_glob() {
        let result = glob_to_regex("src/**/*.rs").expect("should convert complex glob");
        let regex = regex::bytes::Regex::new(&result).expect("should compile to valid regex");
        assert!(regex.is_match(b"src/lib.rs"));
        assert!(regex.is_match(b"src/sub/mod.rs"));
    }

    #[test]
    fn glob_to_regex_handles_question_mark() {
        let result = glob_to_regex("file?.rs").expect("should convert ? glob");
        let regex = regex::bytes::Regex::new(&result).expect("should compile to valid regex");
        assert!(regex.is_match(b"file1.rs"));
        assert!(regex.is_match(b"fileA.rs"));
        assert!(!regex.is_match(b"file12.rs"));
    }

    #[test]
    fn glob_to_regex_handles_brackets() {
        let result = glob_to_regex("file[0-9].rs").expect("should convert bracket glob");
        let regex = regex::bytes::Regex::new(&result).expect("should compile to valid regex");
        assert!(regex.is_match(b"file1.rs"));
        assert!(regex.is_match(b"file9.rs"));
        assert!(!regex.is_match(b"filea.rs"));
    }

    #[test]
    fn glob_to_regex_respects_literal_separator() {
        let result = glob_to_regex("*.rs").expect("should convert glob");
        let regex = regex::bytes::Regex::new(&result).expect("should compile to valid regex");
        assert!(regex.is_match(b"file.rs"));
    }

    #[test]
    fn glob_to_regex_returns_error_for_invalid_glob() {
        let result = glob_to_regex("[invalid");
        assert!(result.is_err(), "invalid glob should return error");
    }
}
