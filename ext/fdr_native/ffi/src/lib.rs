//! Ruby FFI bindings for the fdr-core search library.
#![allow(unsafe_code, reason = "FFI requires unsafe for Ruby interop")]

use fdr_core::{GrepConfig, SearchConfig, SearchError, grep_with_cancel, search_with_cancel};
use magnus::scan_args::scan_args;
use magnus::value::LazyId;
use magnus::{Error, RArray, RHash, Ruby, TryConvert, Value, function, prelude::*};
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::path::PathBuf;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

static PATTERN: LazyId = LazyId::new("pattern");
static PATHS: LazyId = LazyId::new("paths");
static HIDDEN: LazyId = LazyId::new("hidden");
static NO_IGNORE: LazyId = LazyId::new("no_ignore");
static CASE_SENSITIVE: LazyId = LazyId::new("case_sensitive");
static GLOB: LazyId = LazyId::new("glob");
static FULL_PATH: LazyId = LazyId::new("full_path");
static FOLLOW: LazyId = LazyId::new("follow");
static MAX_DEPTH: LazyId = LazyId::new("max_depth");
static MIN_DEPTH: LazyId = LazyId::new("min_depth");
static TYPE: LazyId = LazyId::new("type");
static EXTENSION: LazyId = LazyId::new("extension");
static EXCLUDE: LazyId = LazyId::new("exclude");
static MIN_SIZE: LazyId = LazyId::new("min_size");
static MAX_SIZE: LazyId = LazyId::new("max_size");
static CHANGED_WITHIN: LazyId = LazyId::new("changed_within");
static CHANGED_BEFORE: LazyId = LazyId::new("changed_before");
static NAME: LazyId = LazyId::new("name");

fn extract_optional_arg<T: TryConvert>(hash: RHash, key: &LazyId) -> Result<Option<T>, Error> {
    hash.get(**key)
        .filter(|val| !val.is_nil())
        .map(TryConvert::try_convert)
        .transpose()
}

/// Runs `func` with the GVL released, so it must not touch any Ruby API.
/// Ruby sets `cancel` when the calling thread is interrupted. Returns `None`
/// without running `func` when an interrupt is already pending.
fn without_gvl<F, R>(func: F, cancel: &AtomicBool) -> Option<R>
where
    F: FnOnce() -> R,
{
    struct CallState<F, R> {
        func: Option<F>,
        result: Option<std::thread::Result<R>>,
    }

    unsafe extern "C" fn call<F: FnOnce() -> R, R>(state: *mut c_void) -> *mut c_void {
        // SAFETY: `state` points to the `CallState` below, alive for this synchronous call.
        let state = unsafe { &mut *state.cast::<CallState<F, R>>() };
        if let Some(func) = state.func.take() {
            state.result = Some(catch_unwind(AssertUnwindSafe(func)));
        }
        ptr::null_mut()
    }

    unsafe extern "C" fn interrupt(cancel: *mut c_void) {
        // SAFETY: `cancel` is the `AtomicBool` passed below, alive for the whole call.
        let cancel = unsafe { &*cancel.cast::<AtomicBool>() };
        cancel.store(true, Ordering::Relaxed);
    }

    let mut state = CallState::<F, R> {
        func: Some(func),
        result: None,
    };
    // SAFETY: `call` runs synchronously while `state` is alive, and Ruby may
    // invoke `interrupt` with `cancel` from another thread while it runs.
    unsafe {
        rb_sys::rb_thread_call_without_gvl2(
            Some(call::<F, R>),
            (&raw mut state).cast(),
            Some(interrupt),
            ptr::from_ref(cancel).cast_mut().cast(),
        );
    }
    match state.result {
        Some(Ok(result)) => Some(result),
        Some(Err(panic)) => resume_unwind(panic),
        None => None,
    }
}

/// Raises any pending interrupt, such as `Timeout::Error` or `Interrupt`.
fn check_interrupts() -> Result<(), Error> {
    magnus::rb_sys::protect(|| {
        // SAFETY: called on a Ruby thread with the GVL held.
        unsafe { rb_sys::rb_thread_check_ints() };
        rb_sys::Qnil as rb_sys::VALUE
    })?;
    Ok(())
}

/// Retries transiently interrupted calls, then finishes while holding the GVL
/// rather than raising for a handled signal or retrying forever.
fn interruptible<R>(
    run: impl Fn(&AtomicBool) -> Result<R, SearchError>,
) -> Result<Result<R, SearchError>, Error> {
    const SPURIOUS_RETRIES: usize = 3;

    let cancel = AtomicBool::new(false);
    for _ in 0..SPURIOUS_RETRIES {
        cancel.store(false, Ordering::Relaxed);
        let outcome = without_gvl(|| run(&cancel), &cancel);
        check_interrupts()?;
        match outcome {
            None | Some(Err(SearchError::Cancelled)) => {}
            Some(result) => return Ok(result),
        }
    }

    Ok(run(&AtomicBool::new(false)))
}

struct SearchParams {
    pattern: Option<String>,
    paths: Option<RArray>,
    hidden: Option<bool>,
    no_ignore: Option<bool>,
    case_sensitive: Option<bool>,
    glob: Option<bool>,
    full_path: Option<bool>,
    follow: Option<bool>,
    max_depth: Option<i64>,
    min_depth: Option<i64>,
    file_type: Option<String>,
    extension: Option<String>,
    exclude: Option<RArray>,
    min_size: Option<i64>,
    max_size: Option<i64>,
    changed_within: Option<i64>,
    changed_before: Option<i64>,
}

fn extract_search_params(kwargs: RHash) -> Result<SearchParams, Error> {
    Ok(SearchParams {
        pattern: extract_optional_arg(kwargs, &PATTERN)?,
        paths: extract_optional_arg(kwargs, &PATHS)?,
        hidden: extract_optional_arg(kwargs, &HIDDEN)?,
        no_ignore: extract_optional_arg(kwargs, &NO_IGNORE)?,
        case_sensitive: extract_optional_arg(kwargs, &CASE_SENSITIVE)?,
        glob: extract_optional_arg(kwargs, &GLOB)?,
        full_path: extract_optional_arg(kwargs, &FULL_PATH)?,
        follow: extract_optional_arg(kwargs, &FOLLOW)?,
        max_depth: extract_optional_arg(kwargs, &MAX_DEPTH)?,
        min_depth: extract_optional_arg(kwargs, &MIN_DEPTH)?,
        file_type: extract_optional_arg(kwargs, &TYPE)?,
        extension: extract_optional_arg(kwargs, &EXTENSION)?,
        exclude: extract_optional_arg(kwargs, &EXCLUDE)?,
        min_size: extract_optional_arg(kwargs, &MIN_SIZE)?,
        max_size: extract_optional_arg(kwargs, &MAX_SIZE)?,
        changed_within: extract_optional_arg(kwargs, &CHANGED_WITHIN)?,
        changed_before: extract_optional_arg(kwargs, &CHANGED_BEFORE)?,
    })
}

fn validate_file_type(ruby: &Ruby, file_type: &str) -> Result<(), Error> {
    const FILE_TYPES: [&str; 7] = ["f", "file", "d", "dir", "directory", "l", "symlink"];

    if FILE_TYPES.contains(&file_type) {
        return Ok(());
    }

    Err(Error::new(
        ruby.exception_arg_error(),
        format!(
            "type must be one of {}, got {file_type}",
            FILE_TYPES.join(", ")
        ),
    ))
}

fn build_search_config(ruby: &Ruby, params: SearchParams) -> Result<SearchConfig, Error> {
    let mut config = SearchConfig::default();

    if let Some(pattern) = params.pattern {
        config.pattern = Some(pattern);
    }

    if let Some(paths_array) = params.paths {
        let mut paths_vec = Vec::with_capacity(paths_array.len());
        for path_val in paths_array {
            let path_str: String = TryConvert::try_convert(path_val)?;
            paths_vec.push(PathBuf::from(path_str));
        }
        config.paths = paths_vec;
    }

    if let Some(hidden) = params.hidden {
        config.hidden = hidden;
    }
    if let Some(no_ignore) = params.no_ignore {
        config.no_ignore = no_ignore;
    }
    if let Some(case_sensitive) = params.case_sensitive {
        config.case_sensitive = case_sensitive;
    }
    if let Some(glob) = params.glob {
        config.glob = glob;
    }
    if let Some(full_path) = params.full_path {
        config.full_path = full_path;
    }
    if let Some(follow) = params.follow {
        config.follow = follow;
    }

    if let Some(max_depth) = params.max_depth {
        let max_depth_usize = usize::try_from(max_depth).map_err(|_| {
            Error::new(
                ruby.exception_arg_error(),
                format!("max_depth must be a non-negative integer, got {max_depth}"),
            )
        })?;
        config.max_depth = Some(max_depth_usize);
    }

    if let Some(min_depth) = params.min_depth {
        let min_depth_usize = usize::try_from(min_depth).map_err(|_| {
            Error::new(
                ruby.exception_arg_error(),
                format!("min_depth must be a non-negative integer, got {min_depth}"),
            )
        })?;
        config.min_depth = Some(min_depth_usize);
    }

    if let Some(file_type) = params.file_type {
        validate_file_type(ruby, &file_type)?;
        config.file_type = Some(file_type);
    }

    if let Some(extension) = params.extension {
        config.extension = Some(extension);
    }

    if let Some(exclude_array) = params.exclude {
        let mut excludes = Vec::with_capacity(exclude_array.len());
        for exclude_val in exclude_array {
            excludes.push(TryConvert::try_convert(exclude_val)?);
        }
        config.exclude = excludes;
    }

    if let Some(min_size) = params.min_size {
        let min_size_u64 = u64::try_from(min_size).map_err(|_| {
            Error::new(
                ruby.exception_arg_error(),
                format!("min_size must be a non-negative integer, got {min_size}"),
            )
        })?;
        config.min_size = Some(min_size_u64);
    }

    if let Some(max_size) = params.max_size {
        let max_size_u64 = u64::try_from(max_size).map_err(|_| {
            Error::new(
                ruby.exception_arg_error(),
                format!("max_size must be a non-negative integer, got {max_size}"),
            )
        })?;
        config.max_size = Some(max_size_u64);
    }

    if let Some(changed_within) = params.changed_within {
        if changed_within < 0 {
            return Err(Error::new(
                ruby.exception_arg_error(),
                format!("changed_within must be a non-negative integer, got {changed_within}"),
            ));
        }
        config.changed_within = Some(changed_within);
    }

    if let Some(changed_before) = params.changed_before {
        if changed_before < 0 {
            return Err(Error::new(
                ruby.exception_arg_error(),
                format!("changed_before must be a non-negative integer, got {changed_before}"),
            ));
        }
        config.changed_before = Some(changed_before);
    }

    Ok(config)
}

fn core_error(ruby: &Ruby, operation: &str, error: &SearchError) -> Error {
    match error {
        SearchError::Cancelled => Error::new(
            ruby.exception_runtime_error(),
            format!("{operation} interrupted"),
        ),
        SearchError::InvalidRegex(_) => Error::new(
            ruby.exception_regexp_error(),
            format!("{operation} failed: {error}"),
        ),
        SearchError::Io(_) => Error::new(
            ruby.exception_io_error(),
            format!("{operation} failed: {error}"),
        ),
        SearchError::InvalidInput(_) => Error::new(
            ruby.exception_arg_error(),
            format!("{operation} failed: {error}"),
        ),
    }
}

fn depth_range_is_empty(config: &SearchConfig) -> bool {
    matches!((config.min_depth, config.max_depth), (Some(min), Some(max)) if min > max)
}

fn fdr_search(ruby: &Ruby, args: &[Value]) -> Result<RArray, Error> {
    let args_scan = scan_args::<(), (), (), (), RHash, ()>(args)?;
    let params = extract_search_params(args_scan.keywords)?;
    let config = build_search_config(ruby, params)?;

    if depth_range_is_empty(&config) {
        return Ok(ruby.ary_new());
    }

    let results = interruptible(|cancel| search_with_cancel(&config, cancel))?
        .map_err(|err| core_error(ruby, "Search", &err))?;

    Ok(ruby.ary_from_vec(results))
}

fn fdr_grep(ruby: &Ruby, args: &[Value]) -> Result<RHash, Error> {
    let args_scan = scan_args::<(), (), (), (), RHash, ()>(args)?;
    let kwargs = args_scan.keywords;
    let mut params = extract_search_params(kwargs)?;
    let pattern = params
        .pattern
        .take()
        .ok_or_else(|| Error::new(ruby.exception_arg_error(), "missing keyword: pattern"))?;
    params.pattern = extract_optional_arg(kwargs, &NAME)?;
    let search = build_search_config(ruby, params)?;

    if depth_range_is_empty(&search) {
        return Ok(ruby.hash_new());
    }

    let config = GrepConfig { pattern, search };
    let results = interruptible(|cancel| grep_with_cancel(&config, cancel))?
        .map_err(|err| core_error(ruby, "Grep", &err))?;
    let ruby_results = ruby.hash_new();

    for result in results {
        let line_numbers = ruby.ary_new();
        for line_number in result.line_numbers {
            line_numbers.push(line_number)?;
        }
        ruby_results.aset(ruby.str_new(&result.path), line_numbers)?;
    }

    Ok(ruby_results)
}

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let fdr_module = ruby.define_module("Fdr")?;

    fdr_module.define_singleton_method("native_search", function!(fdr_search, -1))?;
    fdr_module.define_singleton_method("native_grep", function!(fdr_grep, -1))?;

    Ok(())
}
