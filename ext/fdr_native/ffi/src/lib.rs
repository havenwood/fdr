//! Ruby FFI bindings for the fdr-core search library.
#![allow(unsafe_code, reason = "FFI requires unsafe for Ruby interop")]

use fdr_core::{
    FILE_TYPES, GrepConfig, SearchConfig, SearchError, grep_with_cancel, search_with_cancel,
};
use magnus::scan_args::scan_args;
use magnus::value::LazyId;
use magnus::{Error, RArray, RHash, Ruby, TryConvert, Value, function, prelude::*};
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
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
/// Ruby sets `cancel` when the calling thread is interrupted, or runs `func`
/// uninterrupted when no flag is given. Returns `None` without running `func`
/// when an interrupt is already pending.
fn without_gvl<F, R>(func: F, cancel: Option<&AtomicBool>) -> Option<R>
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
    let interrupt_fn: unsafe extern "C" fn(*mut c_void) = interrupt;
    let unblock = cancel.map(|_| interrupt_fn);
    let unblock_arg = cancel.map_or(ptr::null_mut(), |cancel| {
        ptr::from_ref(cancel).cast_mut().cast()
    });
    // SAFETY: `call` runs synchronously while `state` is alive, and Ruby may
    // invoke `interrupt` with `cancel` from another thread while it runs.
    unsafe {
        rb_sys::rb_thread_call_without_gvl2(
            Some(call::<F, R>),
            (&raw mut state).cast(),
            unblock,
            unblock_arg,
        );
    }
    match state.result {
        Some(Ok(result)) => Some(result),
        Some(Err(panic)) => resume_unwind(panic),
        None => None,
    }
}

/// Retries transiently interrupted calls, then finishes uncancellable with
/// the GVL still released, so a pending interrupt raises once the walk ends.
fn interruptible<R>(
    ruby: &Ruby,
    run: impl Fn(&AtomicBool) -> Result<R, SearchError>,
) -> Result<Result<R, SearchError>, Error> {
    const SPURIOUS_RETRIES: usize = 3;

    let cancel = AtomicBool::new(false);
    for _ in 0..SPURIOUS_RETRIES {
        cancel.store(false, Ordering::Relaxed);
        let outcome = without_gvl(|| run(&cancel), Some(&cancel));
        ruby.thread_check_ints()?;
        match outcome {
            None | Some(Err(SearchError::Cancelled)) => {}
            Some(result) => return Ok(result),
        }
    }

    loop {
        let outcome = without_gvl(|| run(&AtomicBool::new(false)), None);
        ruby.thread_check_ints()?;
        if let Some(result) = outcome {
            return Ok(result);
        }
    }
}

fn extract_array<T: TryConvert>(hash: RHash, key: &LazyId) -> Result<Option<Vec<T>>, Error> {
    let Some(array) = extract_optional_arg::<RArray>(hash, key)? else {
        return Ok(None);
    };

    array
        .into_iter()
        .map(TryConvert::try_convert)
        .collect::<Result<Vec<T>, Error>>()
        .map(Some)
}

fn non_negative<T: TryFrom<i64>>(
    ruby: &Ruby,
    key: &str,
    value: Option<i64>,
) -> Result<Option<T>, Error> {
    let Some(number) = value else {
        return Ok(None);
    };

    T::try_from(number)
        .ok()
        .filter(|_| number >= 0)
        .map(Some)
        .ok_or_else(|| {
            Error::new(
                ruby.exception_arg_error(),
                format!("{key} must be a non-negative integer, got {number}"),
            )
        })
}

fn extract_file_type(ruby: &Ruby, kwargs: RHash) -> Result<Option<String>, Error> {
    let file_type: Option<String> = extract_optional_arg(kwargs, &TYPE)?;

    if let Some(ref file_type) = file_type
        && !FILE_TYPES.contains(&file_type.as_str())
    {
        return Err(Error::new(
            ruby.exception_arg_error(),
            format!(
                "type must be one of {}, got {file_type}",
                FILE_TYPES.join(", ")
            ),
        ));
    }

    Ok(file_type)
}

/// Builds a config from `kwargs`, taking `SearchConfig::pattern` from
/// `pattern_key`, which is `:pattern` for search and `:name` for grep.
fn build_search_config(
    ruby: &Ruby,
    kwargs: RHash,
    pattern_key: &LazyId,
) -> Result<SearchConfig, Error> {
    Ok(SearchConfig {
        pattern: extract_optional_arg(kwargs, pattern_key)?,
        // PathBuf conversion accepts any byte sequence on Unix, so
        // non-UTF-8 paths can be searched.
        paths: extract_array(kwargs, &PATHS)?.unwrap_or_default(),
        hidden: extract_optional_arg(kwargs, &HIDDEN)?.unwrap_or_default(),
        no_ignore: extract_optional_arg(kwargs, &NO_IGNORE)?.unwrap_or_default(),
        case_sensitive: extract_optional_arg(kwargs, &CASE_SENSITIVE)?.unwrap_or_default(),
        glob: extract_optional_arg(kwargs, &GLOB)?.unwrap_or_default(),
        full_path: extract_optional_arg(kwargs, &FULL_PATH)?.unwrap_or_default(),
        follow: extract_optional_arg(kwargs, &FOLLOW)?.unwrap_or_default(),
        max_depth: non_negative(ruby, "max_depth", extract_optional_arg(kwargs, &MAX_DEPTH)?)?,
        min_depth: non_negative(ruby, "min_depth", extract_optional_arg(kwargs, &MIN_DEPTH)?)?,
        file_type: extract_file_type(ruby, kwargs)?,
        extension: extract_optional_arg(kwargs, &EXTENSION)?,
        exclude: extract_array(kwargs, &EXCLUDE)?.unwrap_or_default(),
        min_size: non_negative(ruby, "min_size", extract_optional_arg(kwargs, &MIN_SIZE)?)?,
        max_size: non_negative(ruby, "max_size", extract_optional_arg(kwargs, &MAX_SIZE)?)?,
        changed_within: non_negative(
            ruby,
            "changed_within",
            extract_optional_arg(kwargs, &CHANGED_WITHIN)?,
        )?,
        changed_before: non_negative(
            ruby,
            "changed_before",
            extract_optional_arg(kwargs, &CHANGED_BEFORE)?,
        )?,
    })
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
    let config = build_search_config(ruby, args_scan.keywords, &PATTERN)?;

    if depth_range_is_empty(&config) {
        return Ok(ruby.ary_new());
    }

    let results = interruptible(ruby, |cancel| search_with_cancel(&config, cancel))?
        .map_err(|err| core_error(ruby, "Search", &err))?;

    Ok(ruby.ary_from_vec(results))
}

fn fdr_grep(ruby: &Ruby, args: &[Value]) -> Result<RHash, Error> {
    let args_scan = scan_args::<(), (), (), (), RHash, ()>(args)?;
    let kwargs = args_scan.keywords;
    let pattern: String = extract_optional_arg(kwargs, &PATTERN)?
        .ok_or_else(|| Error::new(ruby.exception_arg_error(), "missing keyword: pattern"))?;
    let search = build_search_config(ruby, kwargs, &NAME)?;

    if depth_range_is_empty(&search) {
        return Ok(ruby.hash_new());
    }

    let config = GrepConfig { pattern, search };
    let results = interruptible(ruby, |cancel| grep_with_cancel(&config, cancel))?
        .map_err(|err| core_error(ruby, "Grep", &err))?;
    let ruby_results = ruby.hash_new();

    for result in results {
        ruby_results.aset(
            ruby.str_new(&result.path),
            ruby.ary_from_vec(result.line_numbers),
        )?;
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
