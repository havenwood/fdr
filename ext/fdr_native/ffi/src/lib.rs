//! Ruby FFI bindings for the fdr-core search library.
#![allow(unsafe_code, reason = "FFI requires unsafe for Ruby interop")]

use fdr_core::{
    FILE_TYPES, GrepConfig, SearchConfig, SearchError, grep_with_cancel, search_with_cancel,
};
use magnus::scan_args::scan_args;
use magnus::value::LazyId;
use magnus::{
    Error, ExceptionClass, RArray, RHash, RModule, RString, Ruby, Symbol, TryConvert, Value,
    function, prelude::*,
};
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};

// rb-sys's tracking allocator calls Ruby's GC API. Search workers allocate on
// native threads without the GVL, so the process allocator must remain in use.

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
static IGNORE_ERROR: LazyId = LazyId::new("ignore_error");
static IGNORE_FILE: LazyId = LazyId::new("ignore_file");

/// Rejects before coercion so caller `to_str` and `to_ary` errors remain intact.
fn reject_type(ruby: &Ruby, value: Value, target: &str) -> Error {
    fdr_error(
        ruby,
        "InvalidType",
        ruby.exception_type_error(),
        format!("no implicit conversion of {} into {target}", value.class()),
    )
}

fn coercible(value: Value, coercions: &[&str]) -> Result<bool, Error> {
    for name in coercions {
        if value.respond_to(*name, true)? {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Accepts valid UTF-8 binary strings that magnus cannot transcode while
/// preserving coercion errors.
fn to_utf8(ruby: &Ruby, value: Value, name: &str) -> Result<String, Error> {
    let Some(string) = RString::from_value(value) else {
        return String::try_convert(value);
    };
    let invalid = || {
        fdr_error(
            ruby,
            "InvalidOption",
            ruby.exception_arg_error(),
            format!("{name} must be valid UTF-8"),
        )
    };

    if string.enc_get() == ruby.ascii8bit_encindex() {
        return String::from_utf8(string.to_bytes().into()).map_err(|_| invalid());
    }

    string.to_string().map_err(|_| invalid())
}

fn extract_string(ruby: &Ruby, hash: RHash, key: &LazyId) -> Result<Option<String>, Error> {
    let Some(value) = hash.get(**key).filter(|value| !value.is_nil()) else {
        return Ok(None);
    };

    if !value.is_kind_of(ruby.class_string()) && !coercible(value, &["to_str"])? {
        return Err(reject_type(ruby, value, "String"));
    }

    to_utf8(ruby, value, LazyId::get_inner_with(key, ruby).name()?).map(Some)
}

fn extract_optional_arg<T: TryConvert>(hash: RHash, key: &LazyId) -> Result<Option<T>, Error> {
    hash.get(**key)
        .filter(|val| !val.is_nil())
        .map(TryConvert::try_convert)
        .transpose()
}

fn extract_boolish(hash: RHash, key: &LazyId, default: bool) -> Result<bool, Error> {
    hash.get(**key)
        .map(bool::try_convert)
        .transpose()
        .map(|value| value.unwrap_or(default))
}

/// Runs `func` without the GVL and must not call Ruby. Interrupts call
/// `unblock(arg)`. `None` means `func` never started.
#[allow(
    unsafe_code,
    reason = "MRI's no-GVL API uses raw pointers and C callbacks"
)]
fn without_gvl<F, R, A>(func: F, unblock: unsafe extern "C" fn(*mut c_void), arg: &A) -> Option<R>
where
    F: FnOnce() -> R + Send,
    R: Send,
    A: Sync,
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
    // SAFETY: the callback runs synchronously while `state` is alive. `F`, `R`,
    // and `A` are safe to send or share when Ruby offloads work, and Ruby may
    // invoke `unblock` with `arg` from another thread while the callback runs.
    #[cfg(ruby_gte_3_4)]
    unsafe {
        rb_sys::rb_nogvl(
            Some(call::<F, R>),
            (&raw mut state).cast(),
            Some(unblock),
            ptr::from_ref(arg).cast_mut().cast(),
            (rb_sys::RB_NOGVL_INTR_FAIL | rb_sys::RB_NOGVL_OFFLOAD_SAFE).cast_signed(),
        );
    }
    #[cfg(not(ruby_gte_3_4))]
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

enum Outcome<R> {
    Done(Result<R, SearchError>),
    Panicked(Box<dyn std::any::Any + Send>),
}

/// A pthread condvar rather than a channel: Rust parks on a libdispatch
/// semaphore, which traps if the process forks and the child calls back in.
struct Handoff<R> {
    state: Mutex<(Option<Outcome<R>>, bool)>,
    ready: Condvar,
}

impl<R> Handoff<R> {
    fn new() -> Self {
        Self {
            state: Mutex::new((None, false)),
            ready: Condvar::new(),
        }
    }

    fn finish(&self, outcome: Outcome<R>) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.0 = Some(outcome);
        drop(state);
        self.ready.notify_all();
    }

    fn interrupt(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.1 = true;
        drop(state);
        self.ready.notify_all();
    }

    /// Blocks until the walk finishes or Ruby interrupts, so it must run
    /// without the GVL. `None` means an interrupt, not a result.
    fn wait(&self) -> Option<Outcome<R>> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if let Some(outcome) = state.0.take() {
                return Some(outcome);
            }
            if std::mem::replace(&mut state.1, false) {
                return None;
            }
            state = self
                .ready
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }
}

unsafe extern "C" fn wake<R>(handoff: *mut c_void) {
    // SAFETY: `handoff` points to the `Arc<Handoff>` contents in
    // `interruptible`, which outlives every call Ruby can make here.
    let handoff = unsafe { &*handoff.cast::<Handoff<R>>() };
    handoff.interrupt();
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
    let handoff = Arc::new(Handoff::<R>::new());
    let worker_handoff = Arc::clone(&handoff);
    let worker_cancel = Arc::clone(cancel);

    std::thread::Builder::new()
        .spawn(move || {
            let outcome = catch_unwind(AssertUnwindSafe(|| run(&worker_cancel)));
            worker_handoff.finish(match outcome {
                Ok(result) => Outcome::Done(result),
                Err(panic) => Outcome::Panicked(panic),
            });
        })
        .map_err(|error| {
            fdr_error(
                ruby,
                "IOError",
                ruby.exception_io_error(),
                format!("could not start the search thread: {error}"),
            )
        })?;

    loop {
        let outcome = without_gvl(|| handoff.wait(), wake::<R>, handoff.as_ref());
        ruby.thread_check_ints()?;
        match outcome {
            Some(Some(Outcome::Done(result))) => {
                drop(stop);
                return Ok(result);
            }
            Some(Some(Outcome::Panicked(panic))) => resume_unwind(panic),
            _ => {}
        }
    }
}

fn convert_array<T: TryConvert>(array: RArray) -> Result<Vec<T>, Error> {
    let len = array.len();
    let mut values = Vec::with_capacity(len);

    for index in 0..len {
        values.push(array.entry(index.cast_signed())?);
    }

    Ok(values)
}

/// Rejects non-strings like `paths` so they raise `Fdr::Error`.
fn extract_strings(ruby: &Ruby, hash: RHash, key: &LazyId) -> Result<Vec<String>, Error> {
    let Some(value) = hash.get(**key).filter(|value| !value.is_nil()) else {
        return Ok(Vec::new());
    };

    if !value.is_kind_of(ruby.class_array()) && !coercible(value, &["to_ary"])? {
        return Err(reject_type(ruby, value, "Array"));
    }

    strings_from_array(ruby, RArray::try_convert(value)?, key)
}

fn strings_from_array(ruby: &Ruby, array: RArray, key: &LazyId) -> Result<Vec<String>, Error> {
    let len = array.len();

    for index in 0..len {
        let element: Value = array.entry(index.cast_signed())?;
        if !element.is_kind_of(ruby.class_string()) && !coercible(element, &["to_str"])? {
            return Err(reject_type(ruby, element, "String"));
        }
    }

    let name = LazyId::get_inner_with(key, ruby).name()?;
    let mut values = Vec::with_capacity(len);

    for index in 0..len {
        let element: Value = array.entry(index.cast_signed())?;
        values.push(to_utf8(ruby, element, name)?);
    }

    Ok(values)
}

fn extract_string_or_strings(ruby: &Ruby, hash: RHash, key: &LazyId) -> Result<Vec<String>, Error> {
    let Some(value) = hash.get(**key).filter(|value| !value.is_nil()) else {
        return Ok(Vec::new());
    };

    let name = LazyId::get_inner_with(key, ruby).name()?;

    if value.is_kind_of(ruby.class_string()) || coercible(value, &["to_str"])? {
        return to_utf8(ruby, value, name).map(|value| vec![value]);
    }
    if !value.is_kind_of(ruby.class_array()) && !coercible(value, &["to_ary"])? {
        return Err(reject_type(ruby, value, "String or Array"));
    }

    strings_from_array(ruby, RArray::try_convert(value)?, key)
}

fn extract_paths(ruby: &Ruby, hash: RHash, key: &LazyId) -> Result<Vec<std::path::PathBuf>, Error> {
    let Some(value) = hash.get(**key) else {
        return Ok(Vec::new());
    };

    if !value.is_kind_of(ruby.class_array()) && !coercible(value, &["to_ary"])? {
        return Err(reject_type(ruby, value, "Array"));
    }

    let array = RArray::try_convert(value)?;
    let len = array.len();

    for index in 0..len {
        let element: Value = array.entry(index.cast_signed())?;
        if !element.is_kind_of(ruby.class_string()) && !coercible(element, &["to_str", "to_path"])?
        {
            return Err(reject_type(ruby, element, "String"));
        }
    }

    convert_array(array)
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
        return Err(fdr_error(
            ruby,
            "InvalidType",
            ruby.exception_type_error(),
            format!("no implicit conversion of {} into Integer", value.class()),
        ));
    }

    let number = i64::try_convert(value).map_err(|_| {
        fdr_error(
            ruby,
            "OutOfRange",
            ruby.exception_range_error(),
            "bignum too big to convert into 'long long'".to_owned(),
        )
    })?;

    T::try_from(number)
        .ok()
        .filter(|_| number >= 0)
        .map(Some)
        .ok_or_else(|| {
            fdr_error(
                ruby,
                "InvalidOption",
                ruby.exception_arg_error(),
                format!("{name} must be a non-negative integer, got {number}"),
            )
        })
}

fn file_type_name(ruby: &Ruby, value: Value) -> Result<String, Error> {
    let file_type = if let Some(symbol) = Symbol::from_value(value) {
        symbol.name()?.into_owned()
    } else {
        to_utf8(ruby, value, "type")?
    };

    if FILE_TYPES.contains(&file_type.as_str()) {
        Ok(file_type)
    } else {
        Err(fdr_error(
            ruby,
            "InvalidOption",
            ruby.exception_arg_error(),
            format!(
                "type must be one of {}, got {file_type}",
                FILE_TYPES.join(", ")
            ),
        ))
    }
}

fn extract_file_types(ruby: &Ruby, kwargs: RHash) -> Result<Vec<String>, Error> {
    let Some(value) = kwargs.get(*TYPE).filter(|value| !value.is_nil()) else {
        return Ok(Vec::new());
    };

    if Symbol::from_value(value).is_some()
        || value.is_kind_of(ruby.class_string())
        || coercible(value, &["to_str"])?
    {
        return file_type_name(ruby, value).map(|file_type| vec![file_type]);
    }
    if !value.is_kind_of(ruby.class_array()) && !coercible(value, &["to_ary"])? {
        return Err(reject_type(ruby, value, "String, Symbol, or Array"));
    }

    let array = RArray::try_convert(value)?;
    let len = array.len();
    let mut file_types = Vec::with_capacity(len);
    for index in 0..len {
        let element: Value = array.entry(index.cast_signed())?;
        if Symbol::from_value(element).is_none()
            && !element.is_kind_of(ruby.class_string())
            && !coercible(element, &["to_str"])?
        {
            return Err(reject_type(ruby, element, "String or Symbol"));
        }
        file_types.push(file_type_name(ruby, element)?);
    }

    Ok(file_types)
}

fn build_search_config(
    ruby: &Ruby,
    kwargs: RHash,
    pattern_key: &LazyId,
    file_type: Vec<String>,
) -> Result<SearchConfig, Error> {
    Ok(SearchConfig {
        pattern: extract_string(ruby, kwargs, pattern_key)?,
        paths: extract_paths(ruby, kwargs, &PATHS)?,
        raise_on_error: !extract_boolish(kwargs, &IGNORE_ERROR, true)?,
        ignore_file: extract_paths(ruby, kwargs, &IGNORE_FILE)?,
        hidden: extract_optional_arg(kwargs, &HIDDEN)?.unwrap_or_default(),
        no_ignore: extract_optional_arg(kwargs, &NO_IGNORE)?.unwrap_or_default(),
        case_sensitive: extract_optional_arg(kwargs, &CASE_SENSITIVE)?.unwrap_or_default(),
        glob: extract_optional_arg(kwargs, &GLOB)?.unwrap_or_default(),
        full_path: extract_optional_arg(kwargs, &FULL_PATH)?.unwrap_or_default(),
        follow: extract_optional_arg(kwargs, &FOLLOW)?.unwrap_or_default(),
        max_depth: non_negative(ruby, kwargs, &MAX_DEPTH, "max_depth")?,
        min_depth: non_negative(ruby, kwargs, &MIN_DEPTH, "min_depth")?,
        file_type,
        extension: extract_string_or_strings(ruby, kwargs, &EXTENSION)?,
        exclude: extract_strings(ruby, kwargs, &EXCLUDE)?,
        min_size: non_negative(ruby, kwargs, &MIN_SIZE, "min_size")?,
        max_size: non_negative(ruby, kwargs, &MAX_SIZE, "max_size")?,
        changed_within: non_negative(ruby, kwargs, &CHANGED_WITHIN, "changed_within")?,
        changed_before: non_negative(ruby, kwargs, &CHANGED_BEFORE, "changed_before")?,
    })
}

/// Uses the stdlib parent when the `Fdr` constant is missing.
fn fdr_class(ruby: &Ruby, name: &str, fallback: ExceptionClass) -> ExceptionClass {
    ruby.class_object()
        .const_get::<_, RModule>("Fdr")
        .and_then(|fdr| fdr.const_get::<_, ExceptionClass>(name))
        .unwrap_or(fallback)
}

fn fdr_error(ruby: &Ruby, name: &str, fallback: ExceptionClass, message: String) -> Error {
    Error::new(fdr_class(ruby, name, fallback), message)
}

/// Raw path in the filesystem encoding, or binary under an ASCII locale.
fn path_string(ruby: &Ruby, path: &[u8]) -> RString {
    if !path.is_ascii() && ruby.filesystem_encindex() == ruby.usascii_encindex() {
        return ruby.enc_str_new(path, ruby.ascii8bit_encoding());
    }

    ruby.enc_str_new(path, ruby.filesystem_encoding())
}

/// Raw line in the external encoding, as with `File.readlines`.
fn line_string(ruby: &Ruby, line: &[u8]) -> RString {
    ruby.enc_str_new(line, ruby.default_external_encoding())
}

fn core_error(ruby: &Ruby, operation: &str, error: &SearchError) -> Error {
    match error {
        SearchError::Cancelled => Error::new(
            ruby.exception_runtime_error(),
            format!("{operation} interrupted"),
        ),
        SearchError::InvalidRegex(_) => fdr_error(
            ruby,
            "InvalidPattern",
            ruby.exception_regexp_error(),
            format!("{operation} failed: {error}"),
        ),
        SearchError::Io(_) => fdr_error(
            ruby,
            "IOError",
            ruby.exception_io_error(),
            format!("{operation} failed: {error}"),
        ),
        SearchError::InvalidInput(_) => fdr_error(
            ruby,
            "InvalidOption",
            ruby.exception_arg_error(),
            format!("{operation} failed: {error}"),
        ),
    }
}

fn fdr_search(ruby: &Ruby, args: &[Value]) -> Result<RArray, Error> {
    let args_scan = scan_args::<(), (), (), (), RHash, ()>(args)?;
    let kwargs = args_scan.keywords;
    let file_type = extract_file_types(ruby, kwargs)?;
    let config = build_search_config(ruby, kwargs, &PATTERN, file_type)?;

    let cancel = Arc::new(AtomicBool::new(false));
    let results = interruptible(ruby, &cancel, move |cancel| {
        search_with_cancel(&config, cancel)
    })?
    .map_err(|err| core_error(ruby, "Search", &err))?;
    let array = ruby.ary_new_capa(results.len());

    for path in results {
        array.push(path_string(ruby, &path))?;
    }

    Ok(array)
}

fn fdr_grep(ruby: &Ruby, args: &[Value]) -> Result<RHash, Error> {
    let args_scan = scan_args::<(), (), (), (), RHash, ()>(args)?;
    let kwargs = args_scan.keywords;
    if let Some(value) = kwargs.get(*PATTERN)
        && value.is_nil()
    {
        return Err(reject_type(ruby, value, "String"));
    }
    let pattern: String = extract_string(ruby, kwargs, &PATTERN)?.ok_or_else(|| {
        fdr_error(
            ruby,
            "InvalidOption",
            ruby.exception_arg_error(),
            "missing keyword: pattern".to_owned(),
        )
    })?;
    if kwargs.get(*TYPE).is_some() {
        return Err(fdr_error(
            ruby,
            "InvalidOption",
            ruby.exception_arg_error(),
            "unknown keyword: :type".to_owned(),
        ));
    }
    let search = build_search_config(ruby, kwargs, &NAME, Vec::new())?;
    let content_case_sensitive = extract_boolish(kwargs, &CONTENT_CASE_SENSITIVE, true)?;

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
    let ruby_results = ruby.hash_new_capa(results.len());

    for result in results {
        let lines = ruby.hash_new_capa(result.lines.len());

        for (number, text) in result.lines {
            lines.aset(number, line_string(ruby, &text))?;
        }

        ruby_results.aset(path_string(ruby, &result.path), lines)?;
    }

    Ok(ruby_results)
}

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    // SAFETY: no Ruby VALUE is cached or crosses threads; the walk runs on
    // plain Rust data, which `Send + 'static` on `interruptible` enforces.
    unsafe { rb_sys::rb_ext_ractor_safe(true) };

    let fdr_module = ruby.define_module("Fdr")?;
    let error = fdr_module.define_module("Error")?;

    for (name, superclass) in [
        ("InvalidPattern", ruby.exception_regexp_error()),
        ("InvalidOption", ruby.exception_arg_error()),
        ("InvalidType", ruby.exception_type_error()),
        ("OutOfRange", ruby.exception_range_error()),
        ("IOError", ruby.exception_io_error()),
    ] {
        fdr_module
            .define_error(name, superclass)?
            .include_module(error)?;
    }

    fdr_module.define_singleton_method("native_search", function!(fdr_search, -1))?;
    fdr_module.define_singleton_method("native_grep", function!(fdr_grep, -1))?;

    Ok(())
}
