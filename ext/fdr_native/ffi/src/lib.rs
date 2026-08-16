//! Ruby FFI bindings for the fdr-core search library.
#![allow(unsafe_code, reason = "FFI requires unsafe for Ruby interop")]

use fdr_core::{GrepConfig, SearchConfig, grep, search};
use magnus::scan_args::scan_args;
use magnus::{Error, RArray, RHash, RString, Ruby, TryConvert, Value, function, prelude::*};
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::path::PathBuf;
use std::ptr;

fn extract_optional_arg<T: TryConvert>(
    ruby: &Ruby,
    hash: RHash,
    key: &str,
) -> Result<Option<T>, Error> {
    hash.get(ruby.to_symbol(key))
        .filter(|val| !val.is_nil())
        .map(TryConvert::try_convert)
        .transpose()
}

/// Runs `func` with the GVL released, so it must not touch any Ruby API.
fn without_gvl<F, R>(func: F) -> R
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
    // SAFETY: `call` runs synchronously while `state` is alive. No unblock
    // function, so Ruby just waits for it to return.
    unsafe {
        rb_sys::rb_thread_call_without_gvl(
            Some(call::<F, R>),
            (&raw mut state).cast(),
            None,
            ptr::null_mut(),
        );
    }
    match state.result {
        Some(Ok(result)) => result,
        Some(Err(panic)) => resume_unwind(panic),
        None => unreachable!("rb_thread_call_without_gvl did not invoke its callback"),
    }
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

fn extract_search_params(ruby: &Ruby, kwargs: RHash) -> Result<SearchParams, Error> {
    Ok(SearchParams {
        pattern: extract_optional_arg(ruby, kwargs, "pattern")?,
        paths: extract_optional_arg(ruby, kwargs, "paths")?,
        hidden: extract_optional_arg(ruby, kwargs, "hidden")?,
        no_ignore: extract_optional_arg(ruby, kwargs, "no_ignore")?,
        case_sensitive: extract_optional_arg(ruby, kwargs, "case_sensitive")?,
        glob: extract_optional_arg(ruby, kwargs, "glob")?,
        full_path: extract_optional_arg(ruby, kwargs, "full_path")?,
        follow: extract_optional_arg(ruby, kwargs, "follow")?,
        max_depth: extract_optional_arg(ruby, kwargs, "max_depth")?,
        min_depth: extract_optional_arg(ruby, kwargs, "min_depth")?,
        file_type: extract_optional_arg(ruby, kwargs, "type")?,
        extension: extract_optional_arg(ruby, kwargs, "extension")?,
        exclude: extract_optional_arg(ruby, kwargs, "exclude")?,
        min_size: extract_optional_arg(ruby, kwargs, "min_size")?,
        max_size: extract_optional_arg(ruby, kwargs, "max_size")?,
        changed_within: extract_optional_arg(ruby, kwargs, "changed_within")?,
        changed_before: extract_optional_arg(ruby, kwargs, "changed_before")?,
    })
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

fn depth_range_is_empty(config: &SearchConfig) -> bool {
    matches!((config.min_depth, config.max_depth), (Some(min), Some(max)) if min > max)
}

/// Raw line bytes tagged with the external encoding, as `File.readlines` does.
fn line_string(ruby: &Ruby, line: &[u8]) -> Result<RString, Error> {
    let string = ruby.str_from_slice(line);
    string.enc_associate(ruby.default_external_encoding())?;

    Ok(string)
}

fn fdr_search(ruby: &Ruby, args: &[Value]) -> Result<RArray, Error> {
    let args_scan = scan_args::<(), (), (), (), RHash, ()>(args)?;
    let params = extract_search_params(ruby, args_scan.keywords)?;
    let config = build_search_config(ruby, params)?;

    if depth_range_is_empty(&config) {
        return Ok(ruby.ary_new());
    }

    let results = without_gvl(|| search(&config))
        .map_err(|err| Error::new(ruby.exception_arg_error(), format!("Search failed: {err}")))?;
    let ruby_array = ruby.ary_new();
    for result in results {
        ruby_array.push(ruby.str_new(&result))?;
    }

    Ok(ruby_array)
}

fn fdr_grep(ruby: &Ruby, args: &[Value]) -> Result<RHash, Error> {
    let args_scan = scan_args::<(), (), (), (), RHash, ()>(args)?;
    let kwargs = args_scan.keywords;
    let pattern: String = extract_optional_arg(ruby, kwargs, "pattern")?
        .ok_or_else(|| Error::new(ruby.exception_arg_error(), "missing keyword: pattern"))?;
    let mut params = extract_search_params(ruby, kwargs)?;
    params.pattern = extract_optional_arg(ruby, kwargs, "name")?;
    let search = build_search_config(ruby, params)?;

    if depth_range_is_empty(&search) {
        return Ok(ruby.hash_new());
    }

    let config = GrepConfig { pattern, search };
    let results = without_gvl(|| grep(&config))
        .map_err(|err| Error::new(ruby.exception_arg_error(), format!("Grep failed: {err}")))?;
    let ruby_results = ruby.hash_new_capa(results.len());

    for result in results {
        let lines = ruby.hash_new_capa(result.lines.len());

        for (number, text) in &result.lines {
            lines.aset(*number, line_string(ruby, text)?)?;
        }

        ruby_results.aset(ruby.str_new(&result.path), lines)?;
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
