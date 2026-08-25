use globset::GlobBuilder;
use grep_matcher::Matcher;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder};
use ignore::{DirEntry, WalkBuilder, WalkState};
use regex::bytes::{Regex, RegexBuilder};
use std::io::{self, Read, Seek};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
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
    /// Drops `./` from safe paths under the implicit cwd root.
    pub strip_cwd_prefix: bool,
    /// Inverted so unreadable entries remain skipped by default.
    pub raise_on_error: bool,
    /// Extra gitignore-format files at lowest precedence, even with `no_ignore`.
    pub ignore_file: Vec<PathBuf>,
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
    /// Searches binary content past the first NUL, like `rg -a`.
    pub text: bool,
    pub format: GrepFormat,
    /// File filters, with `SearchConfig::pattern` matching names.
    pub search: SearchConfig,
}

impl Default for GrepConfig {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            content_case_sensitive: true,
            text: false,
            format: GrepFormat::Line,
            search: SearchConfig::default(),
        }
    }
}

impl GrepConfig {
    fn occurrence_mode(&self) -> bool {
        self.format != GrepFormat::Line
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GrepFormat {
    #[default]
    Line,
    Column,
    ByteRange,
}

impl GrepFormat {
    pub fn from_options(column: bool, byte_range: bool) -> Result<Self, SearchError> {
        if column && byte_range {
            return Err(SearchError::InvalidInput(
                "column and byte_range cannot both be true".to_owned(),
            ));
        }

        Ok(if column {
            Self::Column
        } else if byte_range {
            Self::ByteRange
        } else {
            Self::Line
        })
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum GrepPosition {
    Column(u64),
    ByteRange { offset: u64, length: u64 },
}

#[derive(Debug, Eq, PartialEq)]
pub struct GrepMatch {
    pub path: Arc<[u8]>,
    pub line_number: u64,
    pub position: Option<GrepPosition>,
    pub text: Arc<[u8]>,
}

#[derive(Debug)]
pub enum SearchError {
    InvalidRegex(String),
    InvalidInput(String),
    Io(io::Error),
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

impl From<io::Error> for SearchError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl std::error::Error for SearchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

fn build_pattern_regex(config: &SearchConfig) -> Result<Option<Regex>, SearchError> {
    let Some(pat) = &config.pattern else {
        return Ok(None);
    };

    // Treat the empty glob's unmatchable `^$` as no pattern, like fd.
    let glob = if config.glob && !pat.is_empty() {
        Some(build_glob(pat).map_err(|error| SearchError::InvalidInput(error.to_string()))?)
    } else {
        None
    };
    let regex_pattern = glob.as_ref().map_or(pat.as_str(), |glob| glob.regex());

    let regex = RegexBuilder::new(regex_pattern)
        .case_insensitive(!config.case_sensitive)
        .dot_matches_new_line(true)
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
    full_path: bool,
    strip_cwd_prefix: bool,
    full_path_base: Option<PathBuf>,
    follow: bool,
    min_size: Option<u64>,
    max_size: Option<u64>,
    changed_within: Option<std::time::SystemTime>,
    changed_before: Option<std::time::SystemTime>,
}

impl EntryFilters {
    fn new(config: &SearchConfig) -> Result<Self, SearchError> {
        let full_path = config.full_path && config.pattern.is_some();
        let full_path_base = if full_path && config.paths.iter().any(|path| path.is_relative()) {
            Some(std::env::current_dir()?)
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
                .map(|file_type| file_type.parse::<FileTypeFilter>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(SearchError::InvalidInput)?,
            min_depth: config.min_depth,
            full_path,
            strip_cwd_prefix: config.strip_cwd_prefix,
            full_path_base,
            follow: config.follow,
            min_size: config.min_size,
            max_size: config.max_size,
            changed_within: config.changed_within.map(cutoff),
            changed_before: config.changed_before.map(cutoff),
        })
    }

    fn matches(&self, entry: &WalkEntry) -> bool {
        let path = entry.path();

        if let Some(regex) = self.pattern.as_ref() {
            // Raw OS bytes, so non-UTF-8 names match themselves.
            let search_bytes = if self.full_path {
                let relative = path.strip_prefix(".").unwrap_or(path);
                match self.full_path_base.as_deref() {
                    Some(base) if relative.is_relative() => {
                        std::borrow::Cow::Owned(base.join(relative).into_os_string().into_vec())
                    }
                    _ => std::borrow::Cow::Borrowed(relative.as_os_str().as_bytes()),
                }
            } else {
                std::borrow::Cow::Borrowed(path.file_name().unwrap_or_default().as_bytes())
            };
            if !regex.is_match(&search_bytes) {
                return false;
            }
        }

        // Always match extensions against file names, even in full-path mode.
        if let Some(ext_regex) = self.extension.as_ref()
            && !ext_regex.is_match(path.file_name().unwrap_or_default().as_bytes())
        {
            return false;
        }

        if !self.file_type.is_empty()
            && !self.entry_file_type(entry).is_some_and(|entry_file_type| {
                self.file_type
                    .iter()
                    .any(|file_type| file_type.matches(entry_file_type))
            })
        {
            return false;
        }

        self.matches_metadata(entry)
    }

    /// Only roots need a stat because deeper entries already know their type.
    fn entry_file_type(&self, entry: &WalkEntry) -> Option<std::fs::FileType> {
        match entry {
            WalkEntry::Normal(_) if entry.depth() == Some(0) => self
                .entry_metadata(entry)
                .map(|metadata| metadata.file_type()),
            _ => entry.file_type(),
        }
    }

    /// Stats roots by `follow`, since the walkers disagree at depth 0.
    fn entry_metadata(&self, entry: &WalkEntry) -> Option<std::fs::Metadata> {
        match entry {
            WalkEntry::Normal(_) if entry.depth() == Some(0) => {
                let path = entry.path();
                if self.follow {
                    path.metadata().ok()
                } else {
                    path.symlink_metadata().ok()
                }
            }
            _ => entry.metadata(),
        }
    }

    fn matches_metadata(&self, entry: &WalkEntry) -> bool {
        let sized = self.min_size.is_some() || self.max_size.is_some();
        let timed = self.changed_within.is_some() || self.changed_before.is_some();

        if !sized && !timed {
            return true;
        }

        let Some(metadata) = self.entry_metadata(entry) else {
            return false;
        };

        if sized {
            // Identify files through symlinks but measure each entry itself,
            // so only a symlink needs that second stat.
            let file_type = metadata.file_type();
            let is_file = if file_type.is_symlink() {
                entry.path().is_file()
            } else {
                file_type.is_file()
            };
            if !is_file {
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

impl std::str::FromStr for FileTypeFilter {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "f" | "file" => Ok(Self::File),
            "d" | "dir" | "directory" => Ok(Self::Dir),
            "l" | "symlink" => Ok(Self::Symlink),
            _ => Err(format!(
                "file type must be one of {}, got {name}",
                FILE_TYPES.join(", ")
            )),
        }
    }
}

impl FileTypeFilter {
    fn matches(self, file_type: std::fs::FileType) -> bool {
        match self {
            Self::File => file_type.is_file(),
            Self::Dir => file_type.is_dir(),
            Self::Symlink => file_type.is_symlink(),
        }
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

    fn into_path(self) -> PathBuf {
        match self {
            Self::Normal(entry) => entry.into_path(),
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

fn ignore_file_error(error: &ignore::Error) -> SearchError {
    if error.is_io() {
        walk_error(error)
    } else {
        SearchError::InvalidInput(error.to_string())
    }
}

fn walk_entry(
    entry: Result<DirEntry, ignore::Error>,
    raise_on_error: bool,
) -> Result<Option<WalkEntry>, SearchError> {
    let error = match entry {
        Ok(entry) => return Ok(Some(WalkEntry::Normal(entry))),
        Err(error) => error,
    };
    // Build only when the error will be raised, before consuming it.
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

/// Adds `./` to bare `-` because `ignore` treats it as stdin.
fn stdin_safe(path: &Path) -> std::borrow::Cow<'_, Path> {
    if path == Path::new("-") {
        std::borrow::Cow::Owned(PathBuf::from("./-"))
    } else {
        std::borrow::Cow::Borrowed(path)
    }
}

fn depth_range_is_empty(config: &SearchConfig) -> bool {
    matches!((config.min_depth, config.max_depth), (Some(min), Some(max)) if min > max)
}

/// Uses half the cores because directory I/O, not CPU, limits walking.
fn walk_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .div_ceil(2)
}

#[must_use]
pub fn queue_capacity() -> usize {
    walk_threads() * 2
}

/// Rejects relative environment paths to avoid reading from cwd.
fn absolute_env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// `fd`'s global, lowest-precedence ignore file. `rg` has no equivalent.
fn fd_global_ignore() -> Option<PathBuf> {
    let config = absolute_env_path("XDG_CONFIG_HOME")
        .or_else(|| absolute_env_path("HOME").map(|home| home.join(".config")))?;
    let path = config.join("fd").join("ignore");

    path.is_file().then_some(path)
}

fn build_walker(
    config: &SearchConfig,
    tool_ignore_filename: &str,
    global_ignore: Option<&Path>,
) -> Result<Option<WalkBuilder>, SearchError> {
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

    if !config.no_ignore {
        builder.add_custom_ignore_filename(tool_ignore_filename);

        if let Some(path) = global_ignore {
            // A broken global file should not fail every search.
            drop(builder.add_ignore(path));
        }
    }

    // Explicit ignore files still apply with `no_ignore`, like `fd` and `rg`.
    for path in &config.ignore_file {
        if let Some(error) = builder.add_ignore(path)
            && config.raise_on_error
        {
            return Err(ignore_file_error(&error));
        }
    }

    if let Some(overrides) = overrides {
        builder.overrides(overrides);
    }

    Ok(Some(builder))
}

const STREAM_CHUNK: usize = 256;

/// Keeps queued grep text near the same bound even when lines are huge.
const GREP_STREAM_CHUNK_BYTES: usize = 1024 * 1024;

/// `ignore` parallelizes over directories, so entry count is not a useful cue.
const DIRECTORY_THRESHOLD: usize = 64;

fn parallel_search_required(directories: usize) -> bool {
    directories >= DIRECTORY_THRESHOLD
}

/// Limits discarded pre-scan work and first-result buffering.
const SERIAL_PRESCAN_ENTRIES: usize = 512;

/// Limits discarded work during serial grep pre-scan.
const GREP_SERIAL_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// Bounds what the pre-scan buffers, since one line holds as many occurrences
/// as bytes and `GREP_SERIAL_MAX_BYTES` alone would allow millions.
const GREP_SERIAL_MAX_MATCHES: usize = 4096;

/// Grows batches to `STREAM_CHUNK` but sends the first result immediately.
/// `visited` flushes partial batches stranded by selective filters.
struct EmitBatch<'a, T, F: Fn(Vec<T>) -> bool> {
    items: Vec<T>,
    limit: usize,
    weight: usize,
    since_flush: usize,
    emit: &'a F,
    stopped: &'a AtomicBool,
}

impl<'a, T, F: Fn(Vec<T>) -> bool> EmitBatch<'a, T, F> {
    fn new(emit: &'a F, stopped: &'a AtomicBool) -> Self {
        Self {
            items: Vec::new(),
            limit: 1,
            weight: 0,
            since_flush: 0,
            emit,
            stopped,
        }
    }

    fn push(&mut self, item: T) -> bool {
        self.push_weighted(item, 0)
    }

    fn push_weighted(&mut self, item: T, weight: usize) -> bool {
        self.items.push(item);
        self.weight = self.weight.saturating_add(weight);
        (self.items.len() < self.limit && self.weight < GREP_STREAM_CHUNK_BYTES) || self.flush()
    }

    fn visited(&mut self) -> bool {
        self.since_flush += 1;
        self.since_flush < self.limit || self.flush()
    }

    fn flush(&mut self) -> bool {
        self.since_flush = 0;
        if self.items.is_empty() {
            return true;
        }

        let batch = std::mem::take(&mut self.items);
        self.weight = 0;
        self.limit = self.limit.saturating_mul(2).min(STREAM_CHUNK);
        self.items.reserve(self.limit);
        if (self.emit)(batch) {
            return true;
        }

        self.stopped.store(true, Ordering::Relaxed);
        false
    }
}

impl<T, F: Fn(Vec<T>) -> bool> Drop for EmitBatch<'_, T, F> {
    fn drop(&mut self) {
        self.flush();
    }
}

fn search_entry(entry: WalkEntry, filters: &EntryFilters) -> Option<Vec<u8>> {
    // Skip symlinked directory roots the same way in both walkers.
    if entry.depth() == Some(0) && entry.path().is_dir() {
        return None;
    }

    if let Some(min_depth) = filters.min_depth
        && entry.depth().is_none_or(|depth| depth < min_depth)
    {
        return None;
    }

    if !filters.matches(&entry) {
        return None;
    }

    Some(path_into_bytes(entry.into_path(), filters.strip_cwd_prefix))
}

fn serial_search(
    builder: &WalkBuilder,
    filters: &EntryFilters,
    cancel: &AtomicBool,
    raise_on_error: bool,
) -> Result<Option<Vec<Vec<u8>>>, SearchError> {
    let mut results = Vec::new();
    let mut directories = 0;

    for (visited, entry) in builder.build().enumerate() {
        if parallel_search_required(directories) || visited >= SERIAL_PRESCAN_ENTRIES {
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

        if let Some(path) = search_entry(entry, filters) {
            results.push(path);
        }
    }

    if cancel.load(Ordering::Relaxed) {
        return Err(SearchError::Cancelled);
    }

    Ok(Some(results))
}

/// Keeps the first walk error, since the parallel walkers race.
fn record(slot: &Mutex<Option<SearchError>>, error: SearchError) {
    let mut slot = lock(slot);
    if slot.is_none() {
        *slot = Some(error);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Emits path batches. Returning `false` stops early without error.
pub fn search_stream<F>(
    config: &SearchConfig,
    cancel: &AtomicBool,
    emit: F,
) -> Result<(), SearchError>
where
    F: Fn(Vec<Vec<u8>>) -> bool + Sync,
{
    let filters = EntryFilters::new(config)?;
    let Some(builder) = build_walker(config, ".fdignore", fd_global_ignore().as_deref())? else {
        return Ok(());
    };
    if depth_range_is_empty(config) {
        return Ok(());
    }

    // Small trees avoid `ignore`'s thread pool.
    if let Some(results) = serial_search(&builder, &filters, cancel, config.raise_on_error)? {
        if !results.is_empty() {
            emit(results);
        }
        return Ok(());
    }

    let stopped = AtomicBool::new(false);
    let failure = Mutex::new(None);
    builder.build_parallel().run(|| {
        let filters = &filters;
        let stopped = &stopped;
        let failure = &failure;
        let mut batch = EmitBatch::new(&emit, stopped);

        Box::new(move |entry| {
            if cancel.load(Ordering::Relaxed) || stopped.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }

            let entry = match walk_entry(entry, config.raise_on_error) {
                Ok(Some(entry)) => entry,
                Ok(None) => return WalkState::Continue,
                Err(error) => {
                    record(failure, error);
                    stopped.store(true, Ordering::Relaxed);
                    return WalkState::Quit;
                }
            };

            let live = match search_entry(entry, filters) {
                Some(path) => batch.push(path),
                None => batch.visited(),
            };

            if live {
                WalkState::Continue
            } else {
                WalkState::Quit
            }
        })
    });

    if cancel.load(Ordering::Relaxed) {
        return Err(SearchError::Cancelled);
    }
    lock(&failure).take().map_or(Ok(()), Err)
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

/// Drops LF but keeps CR searchable.
fn matching_line(bytes: &[u8]) -> &[u8] {
    bytes.strip_suffix(b"\n").unwrap_or(bytes)
}

/// Drops CR from CRLF but keeps a lone trailing CR.
fn line_text(bytes: &[u8]) -> &[u8] {
    let line = matching_line(bytes);
    if bytes.ends_with(b"\n") {
        line.strip_suffix(b"\r").unwrap_or(line)
    } else {
        line
    }
}

struct LineEmitter<'a, 'b, F: Fn(Vec<GrepMatch>) -> bool> {
    path: Arc<[u8]>,
    cancel: &'a AtomicBool,
    matcher: Option<&'a RegexMatcher>,
    byte_range: bool,
    batch: &'a mut EmitBatch<'b, GrepMatch, F>,
}

impl<F: Fn(Vec<GrepMatch>) -> bool> LineEmitter<'_, '_, F> {
    fn emit(
        &mut self,
        line_number: u64,
        position: Option<GrepPosition>,
        text: Arc<[u8]>,
        weight: usize,
    ) -> bool {
        self.batch.push_weighted(
            GrepMatch {
                path: Arc::clone(&self.path),
                line_number,
                position,
                text,
            },
            weight,
        )
    }
}

impl<F: Fn(Vec<GrepMatch>) -> bool> grep_searcher::Sink for LineEmitter<'_, '_, F> {
    type Error = std::io::Error;

    fn matched(
        &mut self,
        _searcher: &Searcher,
        matched: &grep_searcher::SinkMatch<'_>,
    ) -> std::io::Result<bool> {
        if self.cancel.load(Ordering::Relaxed) || self.batch.stopped.load(Ordering::Relaxed) {
            return Ok(false);
        }

        let Some(line_number) = matched.line_number() else {
            return Ok(true);
        };
        let occurrence = self.matcher.is_some();
        let text: Arc<[u8]> = Arc::from(if occurrence {
            matching_line(matched.bytes())
        } else {
            line_text(matched.bytes())
        });
        let text_bytes = text.len();
        let Some(finder) = self.matcher else {
            return Ok(self.emit(line_number, None, text, text_bytes));
        };

        let mut found_any = false;
        let mut live = true;
        finder
            .find_iter(matching_line(matched.bytes()), |found| {
                let weight = if found_any { 0 } else { text_bytes };
                found_any = true;
                let start = found.start() as u64;
                let position = if self.byte_range {
                    GrepPosition::ByteRange {
                        offset: start,
                        length: (found.end() - found.start()) as u64,
                    }
                } else {
                    GrepPosition::Column(start + 1)
                };
                live = self.emit(line_number, Some(position), Arc::clone(&text), weight);
                live
            })
            .map_err(std::io::Error::other)?;

        debug_assert!(found_any, "a matching line should contain an occurrence");
        Ok(live)
    }
}

fn build_searcher(text: bool) -> Searcher {
    let binary = if text {
        BinaryDetection::none()
    } else {
        BinaryDetection::quit(b'\0')
    };
    SearcherBuilder::new()
        .line_number(true)
        .binary_detection(binary)
        // Keep the file's own bytes: sniffing strips a BOM and transcodes UTF-16.
        .bom_sniffing(false)
        .build()
}

fn build_content_matcher(config: &GrepConfig) -> Result<RegexMatcher, SearchError> {
    let mut builder = RegexMatcherBuilder::new();
    builder
        .case_insensitive(!config.content_case_sensitive)
        .unicode(true)
        .octal(false)
        // `^`/`$` anchor per line. Without this grep-regex reads them as
        // haystack anchors and loses the fast path.
        .multi_line(true)
        .dot_matches_new_line(false)
        // Like `rg`, a lone CR is content and `$` anchors only before LF.
        .line_terminator(Some(b'\n'))
        // Ban NUL only while binary detection stops there.
        .ban_byte((!config.text).then_some(b'\0'));
    builder.build(&config.pattern).map_err(|error| {
        let mut message = error.to_string();
        if message.contains("pattern contains \"\\0\"") {
            message.push_str("; pass `text: true` to search binary content");
        }
        SearchError::InvalidRegex(message)
    })
}

/// Skips a UTF-8 BOM for `^` without transcoding UTF-16.
fn skip_utf8_bom(file: &mut std::fs::File) -> io::Result<()> {
    const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

    let mut head = [0_u8; 3];
    match file.read_exact(&mut head) {
        Ok(()) if head == BOM => Ok(()),
        Ok(()) => file.seek(io::SeekFrom::Start(0)).map(drop),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
            file.seek(io::SeekFrom::Start(0)).map(drop)
        }
        Err(error) => Err(error),
    }
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
            return Err(file_io_error(
                entry.path(),
                &io::Error::other("not a regular file"),
            ));
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

/// Scans until the first NUL, retaining earlier matches like `rg`.
fn grep_file<F: Fn(Vec<GrepMatch>) -> bool>(
    searcher: &mut Searcher,
    matcher: &RegexMatcher,
    path: &Path,
    cancel: &AtomicBool,
    filters: &EntryFilters,
    config: &GrepConfig,
    batch: &mut EmitBatch<'_, GrepMatch, F>,
) -> Result<(), SearchError> {
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if config.search.raise_on_error => {
            return Err(file_io_error(path, &error));
        }
        Err(_) => return Ok(()),
    };
    if let Err(error) = skip_utf8_bom(&mut file) {
        return if config.search.raise_on_error {
            Err(file_io_error(path, &error))
        } else {
            Ok(())
        };
    }

    let reader = CancellableReader {
        inner: file,
        cancel,
    };
    let mut sink = LineEmitter {
        path: Arc::from(emitted_path(path, filters.strip_cwd_prefix).as_slice()),
        cancel,
        matcher: config.occurrence_mode().then_some(matcher),
        byte_range: config.format == GrepFormat::ByteRange,
        batch,
    };

    match searcher.search_reader(matcher, reader, &mut sink) {
        Err(_) if cancel.load(Ordering::Relaxed) => Err(SearchError::Cancelled),
        Err(error) if config.search.raise_on_error => Err(file_io_error(path, &error)),
        Ok(()) | Err(_) => Ok(()),
    }
}

fn file_io_error(path: &Path, error: &io::Error) -> SearchError {
    SearchError::Io(io::Error::new(
        error.kind(),
        format!("{}: {error}", path.display()),
    ))
}

/// Returns `None` when the tree is wide enough for `ignore`'s thread pool.
fn serial_grep(
    builder: &WalkBuilder,
    matcher: &RegexMatcher,
    filters: &EntryFilters,
    config: &GrepConfig,
    cancel: &AtomicBool,
    stopped: &AtomicBool,
) -> Result<Option<Vec<GrepMatch>>, SearchError> {
    let mut searcher = build_searcher(config.text);
    let mut scanned_bytes = 0_u64;
    let collected = Mutex::new(Vec::new());
    let overflowed = AtomicBool::new(false);
    let collect = |batch: Vec<GrepMatch>| {
        let mut buffered = lock(&collected);
        buffered.extend(batch);
        let capped = buffered.len() > GREP_SERIAL_MAX_MATCHES;
        drop(buffered);

        if capped {
            overflowed.store(true, Ordering::Relaxed);
        }
        !capped
    };
    let mut batch = EmitBatch::new(&collect, stopped);

    for (visited, entry) in builder.build().enumerate() {
        if visited >= SERIAL_PRESCAN_ENTRIES {
            return Ok(None);
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(SearchError::Cancelled);
        }

        let Some(entry) = walk_entry(entry, config.search.raise_on_error)? else {
            continue;
        };

        if !grep_candidate(&entry, filters, config.search.raise_on_error)? {
            continue;
        }

        scanned_bytes = scanned_bytes.saturating_add(entry.metadata().map_or(0, |m| m.len()));
        if scanned_bytes > GREP_SERIAL_MAX_BYTES {
            return Ok(None);
        }

        grep_file(
            &mut searcher,
            matcher,
            &entry.into_path(),
            cancel,
            filters,
            config,
            &mut batch,
        )?;

        if overflowed.load(Ordering::Relaxed) {
            return Ok(None);
        }
    }

    if cancel.load(Ordering::Relaxed) {
        return Err(SearchError::Cancelled);
    }

    drop(batch);

    Ok(Some(
        collected
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    ))
}

/// Emits one result per line or occurrence.
pub fn grep_stream<F>(config: &GrepConfig, cancel: &AtomicBool, emit: F) -> Result<(), SearchError>
where
    F: Fn(Vec<GrepMatch>) -> bool + Sync,
{
    let matcher = build_content_matcher(config)?;
    let filters = EntryFilters::new(&config.search)?;
    let Some(builder) = build_walker(&config.search, ".rgignore", None)? else {
        return Ok(());
    };
    if depth_range_is_empty(&config.search) {
        return Ok(());
    }

    let stopped = AtomicBool::new(false);
    // Occurrence mode skips the buffering pre-scan: one file can hold a match
    // per byte, which would both defeat an early stop and outweigh the spawn it
    // saves.
    if !config.occurrence_mode() {
        match serial_grep(&builder, &matcher, &filters, config, cancel, &stopped)? {
            Some(buffered) => {
                if !buffered.is_empty() {
                    emit(buffered);
                }
                return Ok(());
            }
            None => stopped.store(false, Ordering::Relaxed),
        }
    }

    let failure = Mutex::new(None);
    builder.build_parallel().run(|| {
        let matcher = &matcher;
        let filters = &filters;
        let stopped = &stopped;
        let failure = &failure;
        let mut searcher = build_searcher(config.text);
        let mut batch = EmitBatch::new(&emit, stopped);

        Box::new(move |entry| {
            if cancel.load(Ordering::Relaxed) || stopped.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }

            let entry = match walk_entry(entry, config.search.raise_on_error) {
                Ok(Some(entry)) => entry,
                Ok(None) => return WalkState::Continue,
                Err(error) => {
                    record(failure, error);
                    stopped.store(true, Ordering::Relaxed);
                    return WalkState::Quit;
                }
            };
            let candidate = match grep_candidate(&entry, filters, config.search.raise_on_error) {
                Ok(candidate) => candidate,
                Err(error) => {
                    record(failure, error);
                    stopped.store(true, Ordering::Relaxed);
                    return WalkState::Quit;
                }
            };
            if candidate
                && let Err(error) = grep_file(
                    &mut searcher,
                    matcher,
                    &entry.into_path(),
                    cancel,
                    filters,
                    config,
                    &mut batch,
                )
            {
                record(failure, error);
                stopped.store(true, Ordering::Relaxed);
                return WalkState::Quit;
            }

            if cancel.load(Ordering::Relaxed) || stopped.load(Ordering::Relaxed) || !batch.visited()
            {
                WalkState::Quit
            } else {
                WalkState::Continue
            }
        })
    });

    if cancel.load(Ordering::Relaxed) {
        return Err(SearchError::Cancelled);
    }
    lock(&failure).take().map_or(Ok(()), Err)
}

/// Raw OS bytes without consuming the path.
fn emitted_path(path: &Path, strip_cwd_prefix: bool) -> Vec<u8> {
    let bytes = path.as_os_str().as_bytes();
    if strip_cwd_prefix
        && let Some(rest) = bytes.strip_prefix(b"./".as_slice())
        && !rest.starts_with(b"-")
    {
        return rest.to_vec();
    }
    bytes.to_vec()
}

fn path_into_bytes(path: PathBuf, strip_cwd_prefix: bool) -> Vec<u8> {
    let mut bytes = path.into_os_string().into_vec();
    if strip_cwd_prefix && bytes.starts_with(b"./") && bytes.get(2) != Some(&b'-') {
        drop(bytes.drain(..2));
    }
    bytes
}

fn build_glob(glob: &str) -> Result<globset::Glob, globset::Error> {
    GlobBuilder::new(glob).literal_separator(true).build()
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
        let builder = build_walker(&config, ".fdignore", None)
            .expect("should build walker")
            .expect("paths should produce a walker");

        assert!(
            serial_search(&builder, &filters, &AtomicBool::new(false), false)
                .expect("serial pre-scan should succeed")
                .is_none(),
            "the directory threshold should select the parallel walker"
        );

        let results = Mutex::new(Vec::new());
        search_stream(&config, &AtomicBool::new(false), |batch| {
            lock(&results).extend(batch);
            true
        })
        .expect("parallel search should succeed");
        let results = lock(&results);
        assert_eq!(results.len(), DIRECTORY_THRESHOLD * 2 + 1);
        assert!(
            results
                .iter()
                .any(|path| AsRef::<[u8]>::as_ref(path) == dangling.as_os_str().as_bytes())
        );
        drop(results);
    }

    #[test]
    fn cwd_prefix_stripping_preserves_option_shaped_paths() {
        for path in ["./-", "./-rf", "./-dir/file"] {
            assert_eq!(emitted_path(Path::new(path), true), path.as_bytes());
            assert_eq!(path_into_bytes(PathBuf::from(path), true), path.as_bytes());
        }

        assert_eq!(emitted_path(Path::new("./sub/-rf"), true), b"sub/-rf");
        assert_eq!(
            path_into_bytes(PathBuf::from("./sub/-rf"), true),
            b"sub/-rf"
        );
    }

    #[cfg(unix)]
    #[test]
    fn path_into_bytes_preserves_invalid_utf8() {
        use std::os::unix::ffi::OsStringExt;

        let path = PathBuf::from(std::ffi::OsString::from_vec(b"bad\xffname.txt".to_vec()));
        assert_eq!(path_into_bytes(path, false), b"bad\xffname.txt");
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
        let mut searcher = build_searcher(false);
        let stopped = AtomicBool::new(false);
        let drop_batch = |_: Vec<GrepMatch>| true;
        let mut batch = EmitBatch::new(&drop_batch, &stopped);
        let mut sink = LineEmitter {
            path: Arc::from(&b"needle.txt"[..]),
            cancel: &cancel,
            matcher: None,
            byte_range: false,
            batch: &mut batch,
        };

        let error = searcher
            .search_reader(&matcher, reader, &mut sink)
            .expect_err("cancelled reader should stop the search");

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(error.to_string(), "search cancelled");
    }

    #[test]
    fn emit_batch_flushes_the_first_result_immediately_then_grows() {
        let stopped = AtomicBool::new(false);
        let flushed = Mutex::new(Vec::new());
        let record = |batch: Vec<u8>| {
            lock(&flushed).push(batch.len());
            true
        };
        let mut batch = EmitBatch::new(&record, &stopped);

        assert!(batch.push(1));
        assert_eq!(*lock(&flushed), vec![1], "the first result cannot wait");

        for size in [2, 4, 8] {
            for item in 0..size {
                assert!(batch.push(item));
            }
        }

        assert_eq!(*lock(&flushed), vec![1, 2, 4, 8]);
    }

    #[test]
    fn emit_batch_flushes_before_huge_lines_fill_a_count_sized_chunk() {
        let stopped = AtomicBool::new(false);
        let flushed = Mutex::new(Vec::new());
        let record = |batch: Vec<u8>| {
            lock(&flushed).push(batch.len());
            true
        };
        let mut batch = EmitBatch::new(&record, &stopped);

        assert!(batch.push(1));
        assert!(batch.push_weighted(2, GREP_STREAM_CHUNK_BYTES));

        assert_eq!(*lock(&flushed), vec![1, 1]);
    }

    #[test]
    fn emit_batch_flushes_a_partial_batch_a_filter_would_strand() {
        let stopped = AtomicBool::new(false);
        let flushed = Mutex::new(Vec::new());
        let record = |batch: Vec<u8>| {
            lock(&flushed).push(batch.len());
            true
        };
        let mut batch = EmitBatch::new(&record, &stopped);

        assert!(batch.push(1));
        assert!(batch.push(2));
        lock(&flushed).clear();
        assert!(
            lock(&flushed).is_empty(),
            "a partial batch should wait for the rest of its chunk"
        );

        for _ in 0..batch.limit {
            assert!(batch.visited());
        }

        assert_eq!(*lock(&flushed), vec![1]);
    }
}
