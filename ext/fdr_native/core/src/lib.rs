use crossbeam_channel::unbounded;
use globset::GlobBuilder;
use grep_matcher::LineTerminator;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder};
use ignore::{DirEntry, WalkBuilder, WalkState};
use regex::bytes::{Regex, RegexBuilder};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent configuration options with no logical relationship"
)]
pub struct SearchConfig {
    pub pattern: Option<String>,
    pub paths: Vec<PathBuf>,
    /// Inverted so unreadable entries remain skipped by default.
    pub raise_on_error: bool,
    pub hidden: bool,
    pub no_ignore: bool,
    pub case_sensitive: bool,
    pub glob: bool,
    pub full_path: bool,
    pub max_depth: Option<usize>,
    pub min_depth: Option<usize>,
    pub file_type: Vec<String>,
    pub extension: Vec<String>,
    pub exclude: Vec<String>,
    pub follow: bool,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub changed_within: Option<i64>,
    pub changed_before: Option<i64>,
}

#[derive(Debug)]
pub struct GrepConfig {
    pub pattern: String,
    pub content_case_sensitive: bool,
    /// File filters, with `SearchConfig::pattern` matching names.
    pub search: SearchConfig,
}

impl Default for GrepConfig {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            content_case_sensitive: true,
            search: SearchConfig::default(),
        }
    }
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct GrepResult {
    pub path: String,
    /// Matching lines as `(one-based number, text without its terminator)`.
    pub lines: Vec<(u64, Vec<u8>)>,
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
    if config.extension.is_empty() {
        return Ok(None);
    }

    let extensions = config
        .extension
        .iter()
        .map(|ext| ext.trim_start_matches('.'))
        .map(regex::escape)
        .collect::<Vec<_>>()
        .join("|");
    // A bare dotfile like `.rs` is not its own extension, matching fd.
    let pattern = format!(r".\.(?:{extensions})$");
    let regex = RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .build()
        .map_err(|error| SearchError::InvalidInput(error.to_string()))?;

    Ok(Some(regex))
}

struct EntryFilters {
    pattern: Option<Regex>,
    extension: Option<Regex>,
    file_type: Vec<FileTypeFilter>,
    /// Applied after walking so shallow ignored or excluded directories can prune.
    min_depth: Option<usize>,
    /// Base for absolute full-path pattern matching, as in fd.
    full_path_base: Option<PathBuf>,
    min_size: Option<u64>,
    max_size: Option<u64>,
    changed_within: Option<std::time::SystemTime>,
    changed_before: Option<std::time::SystemTime>,
}

impl EntryFilters {
    fn new(config: &SearchConfig) -> Result<Self, SearchError> {
        let full_path_base = if config.full_path && config.pattern.is_some() {
            Some(std::env::current_dir().map_err(SearchError::Io)?)
        } else {
            None
        };
        // Keep time-filter cutoffs fixed for the whole walk.
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
            file_type: config
                .file_type
                .iter()
                .filter_map(|file_type| FileTypeFilter::parse(file_type))
                .collect(),
            min_depth: config.min_depth,
            full_path_base,
            min_size: config.min_size,
            max_size: config.max_size,
            changed_within: config.changed_within.map(cutoff),
            changed_before: config.changed_before.map(cutoff),
        })
    }

    fn matches(&self, entry: &WalkEntry) -> bool {
        let path = entry.path();

        if let Some(regex) = self.pattern.as_ref() {
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
            if !regex.is_match(search_str.as_bytes()) {
                return false;
            }
        }

        // Always match extensions against file names, even in full-path mode.
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

        if !self.file_type.is_empty()
            && !self
                .file_type
                .iter()
                .any(|file_type| file_type.matches(entry))
        {
            return false;
        }

        self.matches_metadata(entry)
    }

    fn matches_metadata(&self, entry: &WalkEntry) -> bool {
        let sized = self.min_size.is_some() || self.max_size.is_some();
        let timed = self.changed_within.is_some() || self.changed_before.is_some();

        if !sized && !timed {
            return true;
        }

        let Some(metadata) = entry.metadata() else {
            return false;
        };

        if sized {
            // Follow symlinks to identify files, but measure each entry, as in fd.
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

pub const FILE_TYPES: [&str; 7] = ["f", "file", "d", "dir", "directory", "l", "symlink"];

#[derive(Clone, Copy)]
enum FileTypeFilter {
    File,
    Dir,
    Symlink,
}

impl FileTypeFilter {
    /// `None` for a name outside `FILE_TYPES`, which leaves entries unfiltered.
    fn parse(name: &str) -> Option<Self> {
        match name {
            "f" | "file" => Some(Self::File),
            "d" | "dir" | "directory" => Some(Self::Dir),
            "l" | "symlink" => Some(Self::Symlink),
            _ => None,
        }
    }

    fn matches(self, entry: &WalkEntry) -> bool {
        entry.file_type().is_some_and(|entry_file_type| match self {
            Self::File => entry_file_type.is_file(),
            Self::Dir => entry_file_type.is_dir(),
            Self::Symlink => entry_file_type.is_symlink(),
        })
    }
}

enum WalkEntry {
    Normal(DirEntry),
    BrokenSymlink {
        path: PathBuf,
        depth: Option<usize>,
        metadata: std::fs::Metadata,
    },
}

impl WalkEntry {
    fn path(&self) -> &Path {
        match self {
            Self::Normal(entry) => entry.path(),
            Self::BrokenSymlink { path, .. } => path,
        }
    }

    fn depth(&self) -> Option<usize> {
        match self {
            Self::Normal(entry) => Some(entry.depth()),
            Self::BrokenSymlink { depth, .. } => *depth,
        }
    }

    fn file_type(&self) -> Option<std::fs::FileType> {
        match self {
            Self::Normal(entry) => entry.file_type(),
            Self::BrokenSymlink { metadata, .. } => Some(metadata.file_type()),
        }
    }

    fn metadata(&self) -> Option<std::fs::Metadata> {
        match self {
            Self::Normal(entry) => entry.metadata().ok(),
            Self::BrokenSymlink { metadata, .. } => Some(metadata.clone()),
        }
    }
}

/// Formats errors without the path `ignore` repeats.
fn walk_error(error: &ignore::Error) -> SearchError {
    let message = match (error, error.io_error()) {
        (ignore::Error::WithPath { path, .. }, Some(io)) => format!("{}: {io}", path.display()),
        _ => error.to_string(),
    };

    SearchError::Io(io::Error::other(message))
}

fn walk_entry(
    entry: Result<DirEntry, ignore::Error>,
    raise_on_error: bool,
) -> Result<Option<WalkEntry>, SearchError> {
    let error = match entry {
        Ok(entry) => return Ok(Some(WalkEntry::Normal(entry))),
        Err(error) => error,
    };
    let raised = raise_on_error.then(|| walk_error(&error));

    broken_symlink_entry(error).map_or_else(
        || raised.map_or(Ok(None), Err),
        |recovered| Ok(Some(recovered)),
    )
}

fn broken_symlink_entry(error: ignore::Error) -> Option<WalkEntry> {
    let depth = error.depth();
    let ignore::Error::WithPath { path, err } = error else {
        return None;
    };

    let kind = err.io_error()?.kind();
    if depth.is_some_and(|depth| depth > 0) && kind != io::ErrorKind::NotFound {
        return None;
    }

    let metadata = path.symlink_metadata().ok()?;
    metadata
        .file_type()
        .is_symlink()
        .then_some(WalkEntry::BrokenSymlink {
            path,
            depth,
            metadata,
        })
}

fn configure_walker(builder: &mut WalkBuilder, config: &SearchConfig) {
    builder
        .hidden(!config.hidden)
        .ignore(!config.no_ignore)
        .git_ignore(!config.no_ignore)
        .git_global(!config.no_ignore)
        .git_exclude(!config.no_ignore)
        .parents(!config.no_ignore)
        .follow_links(config.follow)
        .max_depth(config.max_depth)
        .threads(walk_threads());
}

/// Anchors excludes to the first root because "." would anchor slash patterns to cwd.
fn build_overrides(
    config: &SearchConfig,
    root: &Path,
) -> Result<Option<ignore::overrides::Override>, SearchError> {
    if config.exclude.is_empty() {
        return Ok(None);
    }

    let mut overrides = ignore::overrides::OverrideBuilder::new(root);

    for pattern in &config.exclude {
        // A blank glob becomes a bare `!`, which excludes the whole tree.
        if pattern.trim().is_empty() {
            return Err(SearchError::InvalidInput(
                "exclude patterns cannot be blank".to_owned(),
            ));
        }
        overrides
            .add(&format!("!{pattern}"))
            .map_err(|error| SearchError::InvalidInput(error.to_string()))?;
    }

    overrides
        .build()
        .map(Some)
        .map_err(|error| SearchError::InvalidInput(error.to_string()))
}

fn depth_range_is_empty(config: &SearchConfig) -> bool {
    matches!((config.min_depth, config.max_depth), (Some(min), Some(max)) if min > max)
}

/// Adds `./` to bare `-` because `ignore` treats it as stdin.
fn stdin_safe(path: &Path) -> std::borrow::Cow<'_, Path> {
    if path == Path::new("-") {
        std::borrow::Cow::Owned(PathBuf::from("./-"))
    } else {
        std::borrow::Cow::Borrowed(path)
    }
}

/// Uses half the cores because directory I/O, not CPU, limits walking.
fn walk_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .div_ceil(2)
}

fn build_walker(config: &SearchConfig) -> Result<Option<WalkBuilder>, SearchError> {
    // Build before empty paths return so bad globs still fail.
    let overrides = build_overrides(
        config,
        config
            .paths
            .first()
            .map_or_else(|| Path::new("."), |path| path),
    )?;
    let Some((first_path, rest)) = config.paths.split_first() else {
        return Ok(None);
    };
    let mut builder = WalkBuilder::new(stdin_safe(first_path));

    for path in rest {
        builder.add(stdin_safe(path));
    }

    configure_walker(&mut builder, config);

    if let Some(overrides) = overrides {
        builder.overrides(overrides);
    }

    Ok(Some(builder))
}

/// Batch size for result collection (same as fd's default).
const BATCH_SIZE: usize = 256;

/// `ignore` parallelizes over directories, so entry count is not a useful cue.
const DIRECTORY_THRESHOLD: usize = 64;

fn parallel_search_required(directories: usize) -> bool {
    directories >= DIRECTORY_THRESHOLD
}

/// Grep scans contents per entry, so threads pay off sooner than for search.
const GREP_PARALLEL_THRESHOLD: usize = 512;

/// Limits discarded work during serial grep pre-scan.
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

fn search_entry(entry: &WalkEntry, filters: &EntryFilters) -> Option<String> {
    // Skip symlinked directory roots the same way in both walkers.
    if entry.depth() == Some(0) && entry.path().is_dir() {
        return None;
    }

    if let Some(min_depth) = filters.min_depth
        && entry.depth().is_none_or(|depth| depth < min_depth)
    {
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
    raise_on_error: bool,
) -> Result<Option<Vec<String>>, SearchError> {
    let mut results = Vec::new();
    let mut directories = 0;

    for entry in builder.build() {
        if parallel_search_required(directories) {
            return Ok(None);
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(SearchError::Cancelled);
        }

        let Some(entry) = walk_entry(entry, raise_on_error)? else {
            continue;
        };
        if entry
            .file_type()
            .is_some_and(|file_type| file_type.is_dir())
        {
            directories += 1;
        }

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
    let Some(builder) = build_walker(config)? else {
        return Ok(Vec::new());
    };
    if depth_range_is_empty(config) {
        return Ok(Vec::new());
    }

    if let Some(results) = serial_search(&builder, &filters, cancel, config.raise_on_error)? {
        return Ok(results);
    }

    let filters = Arc::new(filters);
    let (tx, rx) = unbounded();
    let failure = Mutex::new(None);

    let walker = builder.build_parallel();

    walker.run(|| {
        let tx = tx.clone();
        let filters = Arc::clone(&filters);
        let failure = &failure;

        let mut batch = ResultBatch::new(tx);

        Box::new(move |entry| {
            if cancel.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }

            let entry = match walk_entry(entry, config.raise_on_error) {
                Ok(Some(entry)) => entry,
                Ok(None) => return WalkState::Continue,
                Err(error) => {
                    record(failure, error);
                    return WalkState::Quit;
                }
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
    if let Some(error) = lock(&failure).take() {
        return Err(error);
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

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Keeps the first walk error, since the parallel walkers race.
fn record(slot: &Mutex<Option<SearchError>>, error: SearchError) {
    let mut slot = lock(slot);
    if slot.is_none() {
        *slot = Some(error);
    }
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
    lines: Vec<(u64, Vec<u8>)>,
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
            let text = matched.bytes();
            // Only a CR that precedes the LF is a terminator; a trailing CR on
            // an unterminated final line is content.
            let text = text
                .strip_suffix(b"\n")
                .map_or(text, |line| line.strip_suffix(b"\r").unwrap_or(line));
            self.lines.push((line_number, text.to_vec()));
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
        .line_terminator(LineTerminator::crlf())
        .binary_detection(BinaryDetection::quit(b'\0'))
        // Keep the file's own bytes: sniffing strips a BOM and transcodes UTF-16.
        .bom_sniffing(false)
        .build()
}

fn grep_candidate(
    entry: &WalkEntry,
    filters: &EntryFilters,
    raise_on_error: bool,
) -> Result<bool, SearchError> {
    if !entry
        .file_type()
        .is_some_and(|file_type| file_type.is_file())
    {
        if raise_on_error && entry.depth() == Some(0) && !entry.path().is_dir() {
            return Err(SearchError::Io(io::Error::other(format!(
                "{}: not a regular file",
                entry.path().display()
            ))));
        }
        return Ok(false);
    }

    if let Some(min_depth) = filters.min_depth
        && entry.depth().is_none_or(|depth| depth < min_depth)
    {
        return Ok(false);
    }

    Ok(filters.matches(entry))
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
        lines: Vec::new(),
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
        && !collector.lines.is_empty()
    {
        Some(GrepResult {
            path: path_to_string(path),
            lines: collector.lines,
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
    raise_on_error: bool,
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

        let Some(entry) = walk_entry(entry, raise_on_error)? else {
            continue;
        };

        if !grep_candidate(&entry, filters, raise_on_error)? {
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
        .case_insensitive(!config.content_case_sensitive)
        .unicode(true)
        .octal(false)
        .multi_line(true)
        .dot_matches_new_line(false)
        .crlf(true)
        .ban_byte(Some(b'\0'));
    let matcher = matcher_builder
        .build(&config.pattern)
        .map_err(|error| SearchError::InvalidRegex(error.to_string()))?;
    let filters = EntryFilters::new(&config.search)?;
    let Some(builder) = build_walker(&config.search)? else {
        return Ok(Vec::new());
    };
    if depth_range_is_empty(&config.search) {
        return Ok(Vec::new());
    }

    if let Some(results) = serial_grep(
        &builder,
        &matcher,
        &filters,
        cancel,
        config.search.raise_on_error,
    )? {
        return Ok(results);
    }

    let matcher = Arc::new(matcher);
    let filters = Arc::new(filters);
    let (tx, rx) = unbounded();
    let failure = Mutex::new(None);
    let walker = builder.build_parallel();

    walker.run(|| {
        let matcher = Arc::clone(&matcher);
        let filters = Arc::clone(&filters);
        let tx = tx.clone();
        let failure = &failure;
        let mut searcher = build_searcher();

        Box::new(move |entry| {
            if cancel.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }

            let entry = match walk_entry(entry, config.search.raise_on_error) {
                Ok(Some(entry)) => entry,
                Ok(None) => return WalkState::Continue,
                Err(error) => {
                    record(failure, error);
                    return WalkState::Quit;
                }
            };
            let candidate = match grep_candidate(&entry, &filters, config.search.raise_on_error) {
                Ok(candidate) => candidate,
                Err(error) => {
                    record(failure, error);
                    return WalkState::Quit;
                }
            };
            if !candidate {
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
    if let Some(error) = lock(&failure).take() {
        return Err(error);
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
            kept.lines.append(&mut next.lines);
            kept.lines.sort_unstable();
            kept.lines.dedup_by(|next, kept| next.0 == kept.0);
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
    use tempfile::TempDir;

    #[test]
    #[cfg(unix)]
    fn parallel_search_restarts_without_losing_entries() {
        use std::os::unix::ffi::OsStrExt;

        let temp_dir = TempDir::new().expect("should create temp dir");
        let temp_path = temp_dir.path();

        for index in 0..DIRECTORY_THRESHOLD {
            let directory = temp_path.join(format!("dir_{index:04}"));
            std::fs::create_dir(&directory).expect("should create directory");
            std::fs::File::create(directory.join("file.txt")).expect("should create file");
        }
        let dangling = temp_path.join("dangling.txt");
        std::os::unix::fs::symlink("missing_target", &dangling).expect("should create symlink");

        let config = SearchConfig {
            paths: vec![temp_path.to_path_buf()],
            follow: true,
            ..Default::default()
        };
        let filters = EntryFilters::new(&config).expect("should build filters");
        let builder = build_walker(&config)
            .expect("should build walker")
            .expect("paths should produce a walker");

        assert!(
            serial_search(&builder, &filters, &AtomicBool::new(false), false)
                .expect("serial pre-scan should succeed")
                .is_none(),
            "the directory threshold should select the parallel walker"
        );

        let results = search(&config).expect("parallel search should succeed");
        assert_eq!(results.len(), DIRECTORY_THRESHOLD * 2 + 1);
        assert!(
            results
                .iter()
                .any(|path| AsRef::<[u8]>::as_ref(path) == dangling.as_os_str().as_bytes())
        );
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
                lines: vec![(2, b"two".to_vec()), (3, b"three".to_vec())],
            },
            GrepResult {
                path: "a\u{FFFD}.txt".into(),
                lines: vec![(3, b"three".to_vec()), (7, b"seven".to_vec())],
            },
            GrepResult {
                path: "b.txt".into(),
                lines: vec![(1, b"one".to_vec())],
            },
        ];

        merge_colliding_paths(&mut results);

        assert_eq!(
            results,
            vec![
                GrepResult {
                    path: "a\u{FFFD}.txt".into(),
                    lines: vec![
                        (2, b"two".to_vec()),
                        (3, b"three".to_vec()),
                        (7, b"seven".to_vec())
                    ],
                },
                GrepResult {
                    path: "b.txt".into(),
                    lines: vec![(1, b"one".to_vec())],
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
        let matcher = RegexMatcherBuilder::new()
            .build("needle")
            .expect("should compile regex");
        let mut searcher = build_searcher();
        let mut collector = LineCollector {
            lines: Vec::new(),
            binary: false,
        };

        let error = searcher
            .search_reader(&matcher, reader, &mut collector)
            .expect_err("cancelled reader should stop the search");

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(error.to_string(), "search cancelled");
    }
}
