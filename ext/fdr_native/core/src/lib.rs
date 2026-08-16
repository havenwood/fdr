//! File search library in the style of `fd`
use crossbeam_channel::unbounded;
use globset::GlobBuilder;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder};
use ignore::{DirEntry, WalkBuilder, WalkState};
use regex::bytes::{Regex, RegexBuilder};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct GrepResult {
    pub path: String,
    pub line_numbers: Vec<u64>,
}

#[derive(Debug)]
pub enum SearchError {
    InvalidRegex(String),
    InvalidInput(String),
    Io(std::io::Error),
    Cancelled,
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRegex(message) | Self::InvalidInput(message) => {
                formatter.write_str(message)
            }
            Self::Io(error) => error.fmt(formatter),
            Self::Cancelled => formatter.write_str("interrupted"),
        }
    }
}

impl std::error::Error for SearchError {}

fn build_pattern_regex(config: &SearchConfig) -> Result<Option<Regex>, SearchError> {
    let Some(ref pat) = config.pattern else {
        return Ok(None);
    };

    let regex_pattern = if config.glob {
        glob_to_regex(pat).map_err(|error| SearchError::InvalidInput(error.to_string()))?
    } else {
        pat.clone()
    };

    let regex = RegexBuilder::new(&regex_pattern)
        .case_insensitive(!config.case_sensitive)
        .build()
        .map_err(|error| {
            if config.glob {
                SearchError::InvalidInput(error.to_string())
            } else {
                SearchError::InvalidRegex(error.to_string())
            }
        })?;

    Ok(Some(regex))
}

fn build_extension_regex(config: &SearchConfig) -> Result<Option<Regex>, SearchError> {
    let Some(ref ext) = config.extension else {
        return Ok(None);
    };

    let ext = ext.strip_prefix('.').unwrap_or(ext);
    // Require a character before the dot so a bare dotfile like `.rs`
    // is not treated as its own extension, matching fd.
    let pattern = format!(r".\.{}$", regex::escape(ext));
    let regex = RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .build()
        .map_err(|error| SearchError::InvalidInput(error.to_string()))?;

    Ok(Some(regex))
}

/// Per-entry filters shared by `search` and `grep`.
struct EntryFilters {
    pattern: Option<Regex>,
    extension: Option<Regex>,
    file_type: Option<String>,
    /// Base for absolute full-path pattern matching, as in fd.
    full_path_base: Option<PathBuf>,
    min_size: Option<u64>,
    max_size: Option<u64>,
    /// Earliest modification time allowed by `changed_within`.
    changed_within: Option<std::time::SystemTime>,
    /// Latest modification time allowed by `changed_before`.
    changed_before: Option<std::time::SystemTime>,
}

impl EntryFilters {
    fn new(config: &SearchConfig) -> Result<Self, SearchError> {
        let full_path_base = if config.full_path && config.pattern.is_some() {
            Some(std::env::current_dir().map_err(SearchError::Io)?)
        } else {
            None
        };
        // Resolve time filters against one fixed reference so the cutoff
        // cannot drift between entries during a long walk.
        let now = std::time::SystemTime::now();
        let cutoff = |seconds: i64| {
            now.checked_sub(std::time::Duration::from_secs(
                u64::try_from(seconds).unwrap_or(0),
            ))
            .unwrap_or(std::time::UNIX_EPOCH)
        };

        Ok(Self {
            pattern: build_pattern_regex(config)?,
            extension: build_extension_regex(config)?,
            file_type: config.file_type.clone(),
            full_path_base,
            min_size: config.min_size,
            max_size: config.max_size,
            changed_within: config.changed_within.map(cutoff),
            changed_before: config.changed_before.map(cutoff),
        })
    }

    fn matches(&self, entry: &DirEntry) -> bool {
        let path = entry.path();
        let search_str = self.full_path_base.as_deref().map_or_else(
            || path.file_name().unwrap_or_default().to_string_lossy(),
            |base| {
                let relative = path.strip_prefix(".").unwrap_or(path);
                if relative.is_absolute() {
                    relative.to_string_lossy()
                } else {
                    std::borrow::Cow::Owned(base.join(relative).to_string_lossy().into_owned())
                }
            },
        );

        if let Some(regex) = self.pattern.as_ref()
            && !regex.is_match(search_str.as_bytes())
        {
            return false;
        }

        // The extension always matches against the filename, as in fd, even
        // when the pattern matches against the full path.
        if let Some(ext_regex) = self.extension.as_ref()
            && !ext_regex.is_match(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .as_bytes(),
            )
        {
            return false;
        }

        if let Some(ref file_type) = self.file_type
            && !matches_file_type(entry, file_type)
        {
            return false;
        }

        self.matches_metadata(entry)
    }

    fn matches_metadata(&self, entry: &DirEntry) -> bool {
        let sized = self.min_size.is_some() || self.max_size.is_some();
        let timed = self.changed_within.is_some() || self.changed_before.is_some();

        if !sized && !timed {
            return true;
        }

        let Ok(metadata) = entry.metadata() else {
            return false;
        };

        if sized {
            // Size filters apply only to regular files, resolving symlinks
            // for that test but measuring the entry itself, as in fd.
            if !entry.path().is_file() {
                return false;
            }
            if let Some(min) = self.min_size
                && metadata.len() < min
            {
                return false;
            }
            if let Some(max) = self.max_size
                && metadata.len() > max
            {
                return false;
            }
        }

        if timed {
            let Ok(modified) = metadata.modified() else {
                return false;
            };
            if let Some(cutoff) = self.changed_within
                && modified < cutoff
            {
                return false;
            }
            if let Some(cutoff) = self.changed_before
                && modified > cutoff
            {
                return false;
            }
        }

        true
    }
}

/// File type names accepted by `SearchConfig::file_type`.
pub const FILE_TYPES: [&str; 7] = ["f", "file", "d", "dir", "directory", "l", "symlink"];

fn matches_file_type(entry: &DirEntry, file_type: &str) -> bool {
    let entry_file_type = entry.file_type();

    match file_type {
        "f" | "file" => entry_file_type.is_some_and(|t| t.is_file()),
        "d" | "dir" | "directory" => entry_file_type.is_some_and(|t| t.is_dir()),
        "l" | "symlink" => entry_file_type.is_some_and(|t| t.is_symlink()),
        _ => true,
    }
}

fn configure_walker(
    builder: &mut WalkBuilder,
    config: &SearchConfig,
    root: &Path,
) -> Result<(), SearchError> {
    builder
        .hidden(!config.hidden)
        .ignore(!config.no_ignore)
        .git_ignore(!config.no_ignore)
        .git_global(!config.no_ignore)
        .git_exclude(!config.no_ignore)
        .follow_links(config.follow)
        .max_depth(config.max_depth)
        .min_depth(config.min_depth)
        // Match fd's 64-thread cap instead of ignore's default cap of 12.
        .threads(
            std::thread::available_parallelism()
                .map_or(1, std::num::NonZeroUsize::get)
                .min(64),
        );

    if !config.exclude.is_empty() {
        // Exclude globs anchor to the first search path, as in fd; "." would
        // silently anchor slash-containing patterns to the process cwd.
        let mut overrides = ignore::overrides::OverrideBuilder::new(root);
        for pattern in &config.exclude {
            overrides
                .add(&format!("!{pattern}"))
                .map_err(|error| SearchError::InvalidInput(error.to_string()))?;
        }
        builder.overrides(
            overrides
                .build()
                .map_err(|error| SearchError::InvalidInput(error.to_string()))?,
        );
    }

    Ok(())
}

fn build_walker(config: &SearchConfig) -> Result<WalkBuilder, SearchError> {
    let default_path = PathBuf::from(".");
    let (first_path, rest) = config.paths.split_first().unwrap_or((&default_path, &[]));
    let mut builder = WalkBuilder::new(first_path);

    for path in rest {
        builder.add(path);
    }

    configure_walker(&mut builder, config, first_path)?;

    Ok(builder)
}

/// Batch size for result collection (same as fd's default).
const BATCH_SIZE: usize = 256;

/// Small trees avoid the parallel walker's polling overhead.
const PARALLEL_THRESHOLD: usize = 1024;

/// Grep switches earlier because each entry scans file contents.
const GREP_PARALLEL_THRESHOLD: usize = 64;

/// Serial grep's byte limit bounds discarded pre-scan work.
const GREP_SERIAL_MAX_BYTES: u64 = 8 * 1024 * 1024;

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

pub fn search(config: &SearchConfig) -> Result<Vec<String>, SearchError> {
    search_with_cancel(config, &AtomicBool::new(false))
}

/// Path to report for an entry, or `None` when it is filtered out.
fn search_entry(entry: &DirEntry, filters: &EntryFilters) -> Option<String> {
    // Path::is_dir follows symlinks, so a symlink-to-dir root is skipped
    // consistently by the serial and parallel walkers.
    if entry.depth() == 0 && entry.path().is_dir() {
        return None;
    }

    if !filters.matches(entry) {
        return None;
    }

    Some(path_to_string(entry.path()))
}

fn serial_search(
    builder: &WalkBuilder,
    filters: &EntryFilters,
    cancel: &AtomicBool,
) -> Result<Option<Vec<String>>, SearchError> {
    let mut results = Vec::new();

    for (visited, entry) in builder.build().enumerate() {
        if visited >= PARALLEL_THRESHOLD {
            return Ok(None);
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(SearchError::Cancelled);
        }

        let Ok(entry) = entry else {
            continue;
        };

        if let Some(path) = search_entry(&entry, filters) {
            results.push(path);
        }
    }

    if cancel.load(Ordering::Relaxed) {
        return Err(SearchError::Cancelled);
    }

    results.sort_unstable();

    Ok(Some(results))
}

pub fn search_with_cancel(
    config: &SearchConfig,
    cancel: &AtomicBool,
) -> Result<Vec<String>, SearchError> {
    let filters = EntryFilters::new(config)?;
    let builder = build_walker(config)?;

    if let Some(results) = serial_search(&builder, &filters, cancel)? {
        return Ok(results);
    }

    let filters = Arc::new(filters);
    let (tx, rx) = unbounded();

    let walker = builder.build_parallel();

    walker.run(|| {
        let tx = tx.clone();
        let filters = Arc::clone(&filters);

        let mut batch = ResultBatch::new(tx);

        Box::new(move |entry| {
            if cancel.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }

            let Ok(entry) = entry else {
                return WalkState::Continue;
            };

            if let Some(path) = search_entry(&entry, &filters) {
                batch.push(path);
            }

            WalkState::Continue
        })
    });

    drop(tx);
    if cancel.load(Ordering::Relaxed) {
        return Err(SearchError::Cancelled);
    }

    let batches: Vec<Vec<String>> = rx.iter().collect();
    let total_size: usize = batches.iter().map(Vec::len).sum();
    let mut results = Vec::with_capacity(total_size);

    for batch in batches {
        results.extend(batch);
    }

    results.sort_unstable();

    Ok(results)
}

struct CancellableReader<'a, R> {
    inner: R,
    cancel: &'a AtomicBool,
}

impl<R: Read> Read for CancellableReader<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::other("search cancelled"));
        }

        self.inner.read(buffer)
    }
}

struct LineCollector {
    line_numbers: Vec<u64>,
    binary: bool,
}

impl grep_searcher::Sink for LineCollector {
    type Error = std::io::Error;

    fn matched(
        &mut self,
        _searcher: &Searcher,
        matched: &grep_searcher::SinkMatch<'_>,
    ) -> std::io::Result<bool> {
        if let Some(line_number) = matched.line_number() {
            self.line_numbers.push(line_number);
        }
        Ok(true)
    }

    fn binary_data(
        &mut self,
        _searcher: &Searcher,
        _binary_byte_offset: u64,
    ) -> std::io::Result<bool> {
        self.binary = true;
        Ok(false)
    }
}

pub fn grep(config: &GrepConfig) -> Result<Vec<GrepResult>, SearchError> {
    grep_with_cancel(config, &AtomicBool::new(false))
}

fn build_searcher() -> Searcher {
    SearcherBuilder::new()
        .line_number(true)
        .binary_detection(BinaryDetection::quit(b'\0'))
        .build()
}

/// Whether an entry is a file `grep` should scan.
fn grep_candidate(entry: &DirEntry, filters: &EntryFilters) -> bool {
    entry
        .file_type()
        .is_some_and(|file_type| file_type.is_file())
        && filters.matches(entry)
}

/// Matching lines in `path`, or `None` when it is binary, unreadable, cancelled,
/// or has no match.
fn grep_file(
    searcher: &mut Searcher,
    matcher: &RegexMatcher,
    path: &Path,
    cancel: &AtomicBool,
) -> Option<GrepResult> {
    let mut collector = LineCollector {
        line_numbers: Vec::new(),
        binary: false,
    };
    let file = std::fs::File::open(path).ok()?;
    let reader = CancellableReader {
        inner: file,
        cancel,
    };

    if searcher
        .search_reader(matcher, reader, &mut collector)
        .is_ok()
        && !collector.binary
        && !collector.line_numbers.is_empty()
    {
        Some(GrepResult {
            path: path_to_string(path),
            line_numbers: collector.line_numbers,
        })
    } else {
        None
    }
}

fn serial_grep(
    builder: &WalkBuilder,
    matcher: &RegexMatcher,
    filters: &EntryFilters,
    cancel: &AtomicBool,
) -> Result<Option<Vec<GrepResult>>, SearchError> {
    let mut searcher = build_searcher();
    let mut results = Vec::new();
    let mut scanned_bytes = 0_u64;

    for (visited, entry) in builder.build().enumerate() {
        if visited >= GREP_PARALLEL_THRESHOLD {
            return Ok(None);
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(SearchError::Cancelled);
        }

        let Ok(entry) = entry else {
            continue;
        };

        if !grep_candidate(&entry, filters) {
            continue;
        }

        scanned_bytes = scanned_bytes.saturating_add(entry.metadata().map_or(0, |m| m.len()));
        if scanned_bytes > GREP_SERIAL_MAX_BYTES {
            return Ok(None);
        }

        if let Some(result) = grep_file(&mut searcher, matcher, entry.path(), cancel) {
            results.push(result);
        }
    }

    if cancel.load(Ordering::Relaxed) {
        return Err(SearchError::Cancelled);
    }

    results.sort_unstable();
    merge_colliding_paths(&mut results);

    Ok(Some(results))
}

pub fn grep_with_cancel(
    config: &GrepConfig,
    cancel: &AtomicBool,
) -> Result<Vec<GrepResult>, SearchError> {
    let mut matcher_builder = RegexMatcherBuilder::new();
    matcher_builder
        .case_insensitive(!config.search.case_sensitive)
        .line_terminator(Some(b'\n'));
    let matcher = matcher_builder
        .build(&config.pattern)
        .map_err(|error| SearchError::InvalidRegex(error.to_string()))?;
    let filters = EntryFilters::new(&config.search)?;
    let builder = build_walker(&config.search)?;

    if let Some(results) = serial_grep(&builder, &matcher, &filters, cancel)? {
        return Ok(results);
    }

    let matcher = Arc::new(matcher);
    let filters = Arc::new(filters);
    let (tx, rx) = unbounded();
    let walker = builder.build_parallel();

    walker.run(|| {
        let matcher = Arc::clone(&matcher);
        let filters = Arc::clone(&filters);
        let tx = tx.clone();
        let mut searcher = build_searcher();

        Box::new(move |entry| {
            if cancel.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }

            let Ok(entry) = entry else {
                return WalkState::Continue;
            };

            if !grep_candidate(&entry, &filters) {
                return WalkState::Continue;
            }

            if let Some(result) = grep_file(&mut searcher, matcher.as_ref(), entry.path(), cancel) {
                drop(tx.send(result));
            }

            WalkState::Continue
        })
    });

    drop(tx);
    if cancel.load(Ordering::Relaxed) {
        return Err(SearchError::Cancelled);
    }

    let mut results: Vec<GrepResult> = rx.iter().collect();
    results.sort_unstable();
    merge_colliding_paths(&mut results);

    Ok(results)
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn merge_colliding_paths(results: &mut Vec<GrepResult>) {
    results.dedup_by(|next, kept| {
        next.path == kept.path && {
            kept.line_numbers.append(&mut next.line_numbers);
            kept.line_numbers.sort_unstable();
            kept.line_numbers.dedup();
            true
        }
    });
}

fn glob_to_regex(glob: &str) -> Result<String, globset::Error> {
    let glob_pattern = GlobBuilder::new(glob).literal_separator(true).build()?;

    Ok(glob_pattern.regex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_to_string_preserves_utf8() {
        assert_eq!(path_to_string(Path::new("lib/fdr.rb")), "lib/fdr.rb");
    }

    #[cfg(unix)]
    #[test]
    fn path_to_string_replaces_invalid_utf8() {
        use std::os::unix::ffi::OsStrExt;

        let path = Path::new(std::ffi::OsStr::from_bytes(b"bad\xffname.txt"));
        assert_eq!(path_to_string(path), "bad\u{FFFD}name.txt");
    }

    #[test]
    fn merge_colliding_paths_merges_line_numbers_of_equal_paths() {
        let mut results = vec![
            GrepResult {
                path: "a\u{FFFD}.txt".into(),
                line_numbers: vec![2, 3],
            },
            GrepResult {
                path: "a\u{FFFD}.txt".into(),
                line_numbers: vec![3, 7],
            },
            GrepResult {
                path: "b.txt".into(),
                line_numbers: vec![1],
            },
        ];

        merge_colliding_paths(&mut results);

        assert_eq!(
            results,
            vec![
                GrepResult {
                    path: "a\u{FFFD}.txt".into(),
                    line_numbers: vec![2, 3, 7],
                },
                GrepResult {
                    path: "b.txt".into(),
                    line_numbers: vec![1],
                },
            ]
        );
    }

    #[test]
    fn grep_reader_checks_cancellation_between_buffers() {
        struct CancelAfterFirstRead<'a> {
            cancel: &'a AtomicBool,
            emitted: bool,
        }

        impl Read for CancelAfterFirstRead<'_> {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                if self.emitted {
                    return Ok(0);
                }

                self.emitted = true;
                buffer.fill(b'a');
                self.cancel.store(true, Ordering::Relaxed);
                Ok(buffer.len())
            }
        }

        let cancel = AtomicBool::new(false);
        let reader = CancellableReader {
            inner: CancelAfterFirstRead {
                cancel: &cancel,
                emitted: false,
            },
            cancel: &cancel,
        };
        let matcher = grep_regex::RegexMatcherBuilder::new()
            .build("needle")
            .expect("should compile regex");
        let mut searcher = grep_searcher::SearcherBuilder::new()
            .line_number(true)
            .binary_detection(grep_searcher::BinaryDetection::quit(b'\0'))
            .build();
        let mut collector = LineCollector {
            line_numbers: Vec::new(),
            binary: false,
        };

        let error = searcher
            .search_reader(&matcher, reader, &mut collector)
            .expect_err("cancelled reader should stop the search");

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(error.to_string(), "search cancelled");
    }

    #[test]
    fn glob_to_regex_converts_simple_glob() {
        let result = glob_to_regex("*.rs").expect("should convert *.rs glob");
        let regex = Regex::new(&result).expect("should compile to valid regex");
        assert!(regex.is_match(b"file.rs"));
        assert!(!regex.is_match(b"file.toml"));
    }

    #[test]
    fn glob_to_regex_converts_complex_glob() {
        let result = glob_to_regex("src/**/*.rs").expect("should convert complex glob");
        let regex = Regex::new(&result).expect("should compile to valid regex");
        assert!(regex.is_match(b"src/lib.rs"));
        assert!(regex.is_match(b"src/sub/mod.rs"));
    }

    #[test]
    fn glob_to_regex_handles_question_mark() {
        let result = glob_to_regex("file?.rs").expect("should convert ? glob");
        let regex = Regex::new(&result).expect("should compile to valid regex");
        assert!(regex.is_match(b"file1.rs"));
        assert!(regex.is_match(b"fileA.rs"));
        assert!(!regex.is_match(b"file12.rs"));
    }

    #[test]
    fn glob_to_regex_handles_brackets() {
        let result = glob_to_regex("file[0-9].rs").expect("should convert bracket glob");
        let regex = Regex::new(&result).expect("should compile to valid regex");
        assert!(regex.is_match(b"file1.rs"));
        assert!(regex.is_match(b"file9.rs"));
        assert!(!regex.is_match(b"filea.rs"));
    }

    #[test]
    fn glob_to_regex_respects_literal_separator() {
        let result = glob_to_regex("*.rs").expect("should convert glob");
        let regex = Regex::new(&result).expect("should compile to valid regex");
        assert!(regex.is_match(b"file.rs"));
    }

    #[test]
    fn glob_to_regex_returns_error_for_invalid_glob() {
        let result = glob_to_regex("[invalid");
        assert!(result.is_err(), "invalid glob should return error");
    }
}
