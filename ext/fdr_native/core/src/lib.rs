//! File search library in the style of `fd`
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

#[derive(Debug)]
pub struct GrepConfig {
    /// Regex matched against file contents.
    pub pattern: String,
    /// File selection, where `SearchConfig::pattern` matches against filenames.
    pub search: SearchConfig,
}

impl Default for GrepConfig {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            search: SearchConfig {
                case_sensitive: true,
                ..Default::default()
            },
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct GrepResult {
    pub path: String,
    pub line_numbers: Vec<u64>,
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

/// Per-entry filters shared by `search` and `grep`.
struct EntryFilters {
    pattern: Option<regex::bytes::Regex>,
    extension: Option<regex::bytes::Regex>,
    file_type: Option<String>,
    full_path: bool,
    min_size: Option<u64>,
    max_size: Option<u64>,
    changed_within: Option<i64>,
    changed_before: Option<i64>,
}

impl EntryFilters {
    fn new(config: &SearchConfig) -> Result<Self> {
        Ok(Self {
            pattern: build_pattern_regex(config)?,
            extension: build_extension_regex(config)?,
            file_type: config.file_type.clone(),
            full_path: config.full_path,
            min_size: config.min_size,
            max_size: config.max_size,
            changed_within: config.changed_within,
            changed_before: config.changed_before,
        })
    }

    fn matches(&self, entry: &ignore::DirEntry) -> bool {
        let path = entry.path();
        let search_str = if self.full_path {
            path.to_string_lossy()
        } else {
            path.file_name().unwrap_or_default().to_string_lossy()
        };

        if let Some(regex) = self.pattern.as_ref()
            && !regex.is_match(search_str.as_bytes())
        {
            return false;
        }

        if let Some(ext_regex) = self.extension.as_ref()
            && !ext_regex.is_match(search_str.as_bytes())
        {
            return false;
        }

        if let Some(ref file_type) = self.file_type
            && !matches_file_type(entry, file_type)
        {
            return false;
        }

        matches_metadata_filters(
            entry,
            self.min_size,
            self.max_size,
            self.changed_within,
            self.changed_before,
        )
    }
}

fn matches_file_type(entry: &ignore::DirEntry, file_type: &str) -> bool {
    let entry_file_type = entry.file_type();

    match file_type {
        "f" | "file" => entry_file_type.is_some_and(|t| t.is_file()),
        "d" | "dir" | "directory" => entry_file_type.is_some_and(|t| t.is_dir()),
        "l" | "symlink" => entry_file_type.is_some_and(|t| t.is_symlink()),
        _ => true,
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

fn build_walker(config: &SearchConfig) -> Result<ignore::WalkBuilder> {
    use ignore::WalkBuilder;

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

    Ok(builder)
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
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));

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
    use ignore::WalkState;
    use std::sync::Arc;

    let filters = Arc::new(EntryFilters::new(config)?);
    let builder = build_walker(config)?;

    let (tx, rx) = unbounded();

    let walker = builder.build_parallel();

    walker.run(|| {
        let tx = tx.clone();
        let filters = Arc::clone(&filters);

        let mut batch = ResultBatch::new(tx);

        Box::new(move |entry| {
            let Ok(entry) = entry else {
                return WalkState::Continue;
            };

            if entry.depth() == 0 && entry.file_type().is_some_and(|t| t.is_dir()) {
                return WalkState::Continue;
            }

            if !filters.matches(&entry) {
                return WalkState::Continue;
            }

            batch.push(path_to_string(entry.path()));

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

    results.sort_unstable();

    Ok(results)
}

struct LineCollector {
    line_numbers: Vec<u64>,
    binary: bool,
}

impl grep_searcher::Sink for LineCollector {
    type Error = std::io::Error;

    fn matched(
        &mut self,
        _searcher: &grep_searcher::Searcher,
        matched: &grep_searcher::SinkMatch<'_>,
    ) -> std::io::Result<bool> {
        if let Some(line_number) = matched.line_number() {
            self.line_numbers.push(line_number);
        }
        Ok(true)
    }

    fn binary_data(
        &mut self,
        _searcher: &grep_searcher::Searcher,
        _binary_byte_offset: u64,
    ) -> std::io::Result<bool> {
        self.binary = true;
        Ok(false)
    }
}

pub fn grep(config: &GrepConfig) -> Result<Vec<GrepResult>> {
    use crossbeam_channel::unbounded;
    use grep_regex::RegexMatcherBuilder;
    use grep_searcher::{BinaryDetection, SearcherBuilder};
    use ignore::WalkState;
    use std::sync::Arc;

    let mut matcher_builder = RegexMatcherBuilder::new();
    matcher_builder
        .case_insensitive(!config.search.case_sensitive)
        .line_terminator(Some(b'\n'));
    let matcher = Arc::new(matcher_builder.build(&config.pattern)?);
    let filters = Arc::new(EntryFilters::new(&config.search)?);
    let builder = build_walker(&config.search)?;

    let (tx, rx) = unbounded();
    let walker = builder.build_parallel();

    walker.run(|| {
        let matcher = Arc::clone(&matcher);
        let filters = Arc::clone(&filters);
        let tx = tx.clone();
        let mut searcher = SearcherBuilder::new()
            .line_number(true)
            .binary_detection(BinaryDetection::quit(b'\0'))
            .build();

        Box::new(move |entry| {
            let Ok(entry) = entry else {
                return WalkState::Continue;
            };
            if !entry
                .file_type()
                .is_some_and(|file_type| file_type.is_file())
            {
                return WalkState::Continue;
            }

            if !filters.matches(&entry) {
                return WalkState::Continue;
            }

            let path = entry.path();
            let mut collector = LineCollector {
                line_numbers: Vec::new(),
                binary: false,
            };

            if searcher
                .search_path(matcher.as_ref(), path, &mut collector)
                .is_ok()
                && !collector.binary
                && !collector.line_numbers.is_empty()
            {
                drop(tx.send(GrepResult {
                    path: path_to_string(path),
                    line_numbers: collector.line_numbers,
                }));
            }

            WalkState::Continue
        })
    });

    drop(tx);
    let mut results: Vec<GrepResult> = rx.iter().collect();
    results.sort_unstable_by(|a, b| a.path.cmp(&b.path));

    Ok(results)
}

fn path_to_string(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
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
    fn path_to_string_preserves_utf8() {
        assert_eq!(
            path_to_string(std::path::Path::new("lib/fdr.rb")),
            "lib/fdr.rb"
        );
    }

    #[cfg(unix)]
    #[test]
    fn path_to_string_replaces_invalid_utf8() {
        use std::os::unix::ffi::OsStrExt;

        let path = std::path::Path::new(std::ffi::OsStr::from_bytes(b"bad\xffname.txt"));
        assert_eq!(path_to_string(path), "bad\u{FFFD}name.txt");
    }

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
