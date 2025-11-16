//! Ruby FFI bindings for the fdr-core search library.
#![allow(unsafe_code, reason = "FFI requires unsafe for Ruby interop")]

use fdr_core::{SearchConfig, search};
use magnus::scan_args::scan_args;
use magnus::{Error, RArray, RHash, Ruby, TryConvert, Value, function, prelude::*};
use std::path::PathBuf;

fn extract_optional_arg<T: TryConvert>(ruby: &Ruby, hash: RHash, key: &str) -> Option<T> {
    hash.get(ruby.to_symbol(key)).and_then(|val| {
        if val.is_nil() {
            None
        } else {
            TryConvert::try_convert(val).ok()
        }
    })
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

fn extract_search_params(ruby: &Ruby, kwargs: RHash) -> SearchParams {
    SearchParams {
        pattern: extract_optional_arg(ruby, kwargs, "pattern"),
        paths: extract_optional_arg(ruby, kwargs, "paths"),
        hidden: extract_optional_arg(ruby, kwargs, "hidden"),
        no_ignore: extract_optional_arg(ruby, kwargs, "no_ignore"),
        case_sensitive: extract_optional_arg(ruby, kwargs, "case_sensitive"),
        glob: extract_optional_arg(ruby, kwargs, "glob"),
        full_path: extract_optional_arg(ruby, kwargs, "full_path"),
        follow: extract_optional_arg(ruby, kwargs, "follow"),
        max_depth: extract_optional_arg(ruby, kwargs, "max_depth"),
        min_depth: extract_optional_arg(ruby, kwargs, "min_depth"),
        file_type: extract_optional_arg(ruby, kwargs, "type"),
        extension: extract_optional_arg(ruby, kwargs, "extension"),
        exclude: extract_optional_arg(ruby, kwargs, "exclude"),
        min_size: extract_optional_arg(ruby, kwargs, "min_size"),
        max_size: extract_optional_arg(ruby, kwargs, "max_size"),
        changed_within: extract_optional_arg(ruby, kwargs, "changed_within"),
        changed_before: extract_optional_arg(ruby, kwargs, "changed_before"),
    }
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

fn fdr_search(ruby: &Ruby, args: &[Value]) -> Result<RArray, Error> {
    let args_scan = scan_args::<(), (), (), (), RHash, ()>(args)?;
    let params = extract_search_params(ruby, args_scan.keywords);
    let config = build_search_config(ruby, params)?;

    if let (Some(min), Some(max)) = (config.min_depth, config.max_depth)
        && min > max
    {
        return Ok(ruby.ary_new());
    }

    let results = search(&config).map_err(|err| {
        Error::new(
            ruby.exception_runtime_error(),
            format!("Search failed: {err}"),
        )
    })?;
    let ruby_array = ruby.ary_new();
    for result in results {
        ruby_array.push(ruby.str_new(&result))?;
    }

    Ok(ruby_array)
}

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let fdr_module = ruby.define_module("Fdr")?;

    fdr_module.define_singleton_method("native_search", function!(fdr_search, -1))?;

    Ok(())
}
