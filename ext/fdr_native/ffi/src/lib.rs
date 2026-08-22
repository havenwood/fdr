//! Ruby FFI bindings for the fdr-core search library.
#![allow(unsafe_code, reason = "FFI requires unsafe for Ruby interop")]

use fdr_core::{
    FILE_TYPES, GrepConfig, SearchConfig, SearchError, grep_with_cancel, search_with_cancel,
};
use magnus::scan_args::scan_args;
use magnus::value::LazyId;
use magnus::{Error, RArray, RHash, Ruby, Symbol, TryConvert, Value, function, prelude::*};
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

static PATTERN: LazyId = LazyId::new("pattern");
static PATHS: LazyId = LazyId::new("paths");
static HIDDEN: LazyId = LazyId::new("hidden");
static NO_IGNORE: LazyId = LazyId::new("no_ignore");
static CASE_SENSITIVE: LazyId = LazyId::new("case_sensitive");
static CONTENT_CASE_SENSITIVE: LazyId = LazyId::new("content_case_sensitive");
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

/// Runs `func` without the GVL, so it must not call Ruby. Ruby calls
/// `unblock` with `arg` on interrupt. `None` means `func` never started.
fn without_gvl<F, R, A>(func: F, unblock: unsafe extern "C" fn(*mut c_void), arg: &A) -> Option<R>
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

    let mut state = CallState::<F, R> {
        func: Some(func),
        result: None,
    };
    // SAFETY: `call` runs synchronously while `state` is alive, and Ruby may
    // invoke `unblock` with `arg` from another thread while it runs.
    unsafe {
        rb_sys::rb_thread_call_without_gvl2(
            Some(call::<F, R>),
            (&raw mut state).cast(),
            Some(unblock),
            ptr::from_ref(arg).cast_mut().cast(),
        );
    }
    match state.result {
        Some(Ok(result)) => Some(result),
        Some(Err(panic)) => resume_unwind(panic),
        None => None,
    }
}

enum Message<R> {
    Done(Result<R, SearchError>),
    Panicked(Box<dyn std::any::Any + Send>),
    Wake,
}

unsafe extern "C" fn wake<R>(sender: *mut c_void) {
    // SAFETY: `sender` points to the `Sender` in `interruptible`, which
    // outlives every call Ruby can make here.
    let sender = unsafe { &*sender.cast::<mpsc::Sender<Message<R>>>() };
    drop(sender.send(Message::Wake));
}

/// Waits on a worker thread with the GVL released, so a real interrupt
/// raises and a spurious one resumes the wait without discarding the walk.
fn interruptible<R: Send + 'static>(
    ruby: &Ruby,
    cancel: &Arc<AtomicBool>,
    run: impl FnOnce(&AtomicBool) -> Result<R, SearchError> + Send + 'static,
) -> Result<Result<R, SearchError>, Error> {
    struct StopWorker<'a>(&'a AtomicBool);

    impl Drop for StopWorker<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    let stop = StopWorker(cancel);
    let (sender, receiver) = mpsc::channel::<Message<R>>();
    let worker_sender = sender.clone();
    let worker_cancel = Arc::clone(cancel);

    std::thread::spawn(move || {
        let outcome = catch_unwind(AssertUnwindSafe(|| run(&worker_cancel)));
        drop(worker_sender.send(match outcome {
            Ok(result) => Message::Done(result),
            Err(panic) => Message::Panicked(panic),
        }));
    });

    loop {
        let outcome = without_gvl(|| receiver.recv().ok(), wake::<R>, &sender);
        ruby.thread_check_ints()?;
        match outcome {
            Some(Some(Message::Done(result))) => {
                drop(stop);
                return Ok(result);
            }
            Some(Some(Message::Panicked(panic))) => resume_unwind(panic),
            _ => {}
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

fn extract_paths(hash: RHash) -> Result<Vec<std::path::PathBuf>, Error> {
    let Some(value) = hash.get(*PATHS) else {
        return Ok(Vec::new());
    };
    let array = RArray::try_convert(value)?;

    array
        .into_iter()
        .map(TryConvert::try_convert)
        .collect::<Result<Vec<_>, Error>>()
}

/// Extracts an optional non-negative `Integer`, rejecting `Float` and other
/// numerics that `i64` conversion would silently truncate.
fn non_negative<T: TryFrom<i64>>(
    ruby: &Ruby,
    kwargs: RHash,
    key: &LazyId,
    name: &str,
) -> Result<Option<T>, Error> {
    let Some(value) = kwargs.get(**key).filter(|value| !value.is_nil()) else {
        return Ok(None);
    };

    if !value.is_kind_of(ruby.class_integer()) {
        return Err(Error::new(
            ruby.exception_type_error(),
            format!("no implicit conversion of {} into Integer", value.class()),
        ));
    }

    let number = i64::try_convert(value)?;
    T::try_from(number)
        .ok()
        .filter(|_| number >= 0)
        .map(Some)
        .ok_or_else(|| {
            Error::new(
                ruby.exception_arg_error(),
                format!("{name} must be a non-negative integer, got {number}"),
            )
        })
}

fn extract_file_type(ruby: &Ruby, kwargs: RHash) -> Result<Option<String>, Error> {
    let Some(value) = kwargs.get(*TYPE).filter(|value| !value.is_nil()) else {
        return Ok(None);
    };
    let file_type = if let Some(symbol) = Symbol::from_value(value) {
        symbol.name()?.into_owned()
    } else {
        String::try_convert(value)?
    };

    if !FILE_TYPES.contains(&file_type.as_str()) {
        return Err(Error::new(
            ruby.exception_arg_error(),
            format!(
                "type must be one of {}, got {file_type}",
                FILE_TYPES.join(", ")
            ),
        ));
    }

    Ok(Some(file_type))
}

/// Builds a config from `kwargs`, taking `SearchConfig::pattern` from
/// `pattern_key`, which is `:pattern` for search and `:name` for grep.
fn build_search_config(
    ruby: &Ruby,
    kwargs: RHash,
    pattern_key: &LazyId,
    file_type: Option<String>,
) -> Result<SearchConfig, Error> {
    Ok(SearchConfig {
        pattern: extract_optional_arg(kwargs, pattern_key)?,
        // PathBuf conversion accepts any byte sequence on Unix, so
        // non-UTF-8 paths can be searched.
        paths: extract_paths(kwargs)?,
        hidden: extract_optional_arg(kwargs, &HIDDEN)?.unwrap_or_default(),
        no_ignore: extract_optional_arg(kwargs, &NO_IGNORE)?.unwrap_or_default(),
        case_sensitive: extract_optional_arg(kwargs, &CASE_SENSITIVE)?.unwrap_or_default(),
        glob: extract_optional_arg(kwargs, &GLOB)?.unwrap_or_default(),
        full_path: extract_optional_arg(kwargs, &FULL_PATH)?.unwrap_or_default(),
        follow: extract_optional_arg(kwargs, &FOLLOW)?.unwrap_or_default(),
        max_depth: non_negative(ruby, kwargs, &MAX_DEPTH, "max_depth")?,
        min_depth: non_negative(ruby, kwargs, &MIN_DEPTH, "min_depth")?,
        file_type,
        extension: extract_optional_arg(kwargs, &EXTENSION)?,
        exclude: extract_array(kwargs, &EXCLUDE)?.unwrap_or_default(),
        min_size: non_negative(ruby, kwargs, &MIN_SIZE, "min_size")?,
        max_size: non_negative(ruby, kwargs, &MAX_SIZE, "max_size")?,
        changed_within: non_negative(ruby, kwargs, &CHANGED_WITHIN, "changed_within")?,
        changed_before: non_negative(ruby, kwargs, &CHANGED_BEFORE, "changed_before")?,
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

fn fdr_search(ruby: &Ruby, args: &[Value]) -> Result<RArray, Error> {
    let args_scan = scan_args::<(), (), (), (), RHash, ()>(args)?;
    let kwargs = args_scan.keywords;
    let file_type = extract_file_type(ruby, kwargs)?;
    let config = build_search_config(ruby, kwargs, &PATTERN, file_type)?;

    let cancel = Arc::new(AtomicBool::new(false));
    let results = interruptible(ruby, &cancel, move |cancel| {
        search_with_cancel(&config, cancel)
    })?
    .map_err(|err| core_error(ruby, "Search", &err))?;

    Ok(ruby.ary_from_vec(results))
}

fn fdr_grep(ruby: &Ruby, args: &[Value]) -> Result<RHash, Error> {
    let args_scan = scan_args::<(), (), (), (), RHash, ()>(args)?;
    let kwargs = args_scan.keywords;
    let pattern: String = extract_optional_arg(kwargs, &PATTERN)?
        .ok_or_else(|| Error::new(ruby.exception_arg_error(), "missing keyword: pattern"))?;
    if kwargs.get(*TYPE).is_some() {
        return Err(Error::new(
            ruby.exception_arg_error(),
            "unknown keyword: :type",
        ));
    }
    let search = build_search_config(ruby, kwargs, &NAME, None)?;
    let content_case_sensitive =
        extract_optional_arg(kwargs, &CONTENT_CASE_SENSITIVE)?.unwrap_or(true);

    let config = GrepConfig {
        pattern,
        content_case_sensitive,
        search,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let results = interruptible(ruby, &cancel, move |cancel| {
        grep_with_cancel(&config, cancel)
    })?
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
