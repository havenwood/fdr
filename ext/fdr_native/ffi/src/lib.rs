//! Ruby FFI bindings for the fdr-core search library.
#![allow(unsafe_code, reason = "FFI requires unsafe for Ruby interop")]

use fdr_core::{SearchConfig, search};
use magnus::{Error, RArray, RHash, Ruby, TryConvert, function, prelude::*};
use std::path::PathBuf;

fn extract_bool(ruby: &Ruby, options: RHash, key: &str) -> Result<Option<bool>, Error> {
    if let Some(val) = options.get(ruby.to_symbol(key))
        && !val.is_nil()
    {
        Ok(Some(TryConvert::try_convert(val)?))
    } else {
        Ok(None)
    }
}

fn extract_depth(ruby: &Ruby, options: RHash, key: &str) -> Result<Option<usize>, Error> {
    if let Some(val) = options.get(ruby.to_symbol(key))
        && !val.is_nil()
    {
        let depth: i64 = TryConvert::try_convert(val)?;
        let depth_usize = usize::try_from(depth).map_err(|_| {
            Error::new(
                ruby.exception_arg_error(),
                format!("{key} must be a non-negative integer, got {depth}"),
            )
        })?;
        Ok(Some(depth_usize))
    } else {
        Ok(None)
    }
}

fn extract_size(ruby: &Ruby, options: RHash, key: &str) -> Result<Option<u64>, Error> {
    if let Some(val) = options.get(ruby.to_symbol(key))
        && !val.is_nil()
    {
        let size: i64 = TryConvert::try_convert(val)?;
        let size_u64 = u64::try_from(size).map_err(|_| {
            Error::new(
                ruby.exception_arg_error(),
                format!("{key} must be a non-negative integer, got {size}"),
            )
        })?;
        Ok(Some(size_u64))
    } else {
        Ok(None)
    }
}

fn extract_time(ruby: &Ruby, options: RHash, key: &str) -> Result<Option<i64>, Error> {
    if let Some(val) = options.get(ruby.to_symbol(key))
        && !val.is_nil()
    {
        let seconds: i64 = TryConvert::try_convert(val)?;
        if seconds < 0 {
            return Err(Error::new(
                ruby.exception_arg_error(),
                format!("{key} must be a non-negative integer, got {seconds}"),
            ));
        }
        Ok(Some(seconds))
    } else {
        Ok(None)
    }
}

fn hash_to_config(ruby: &Ruby, options: RHash) -> Result<SearchConfig, Error> {
    let mut config = SearchConfig::default();

    if let Some(val) = options.get(ruby.to_symbol("pattern"))
        && !val.is_nil()
    {
        let pattern: String = TryConvert::try_convert(val)?;
        config.pattern = Some(pattern);
    }

    if let Some(val) = options.get(ruby.to_symbol("paths"))
        && !val.is_nil()
    {
        let paths_array: RArray = TryConvert::try_convert(val)?;
        let mut paths = Vec::with_capacity(paths_array.len());
        for path_val in paths_array {
            let path_str: String = TryConvert::try_convert(path_val)?;
            paths.push(PathBuf::from(path_str));
        }
        config.paths = paths;
    }

    if let Some(val) = extract_bool(ruby, options, "hidden")? {
        config.hidden = val;
    }
    if let Some(val) = extract_bool(ruby, options, "no_ignore")? {
        config.no_ignore = val;
    }
    if let Some(val) = extract_bool(ruby, options, "case_sensitive")? {
        config.case_sensitive = val;
    }
    if let Some(val) = extract_bool(ruby, options, "glob")? {
        config.glob = val;
    }
    if let Some(val) = extract_bool(ruby, options, "full_path")? {
        config.full_path = val;
    }
    if let Some(val) = extract_bool(ruby, options, "follow")? {
        config.follow = val;
    }

    config.max_depth = extract_depth(ruby, options, "max_depth")?;
    config.min_depth = extract_depth(ruby, options, "min_depth")?;

    if let Some(val) = options.get(ruby.to_symbol("type"))
        && !val.is_nil()
    {
        config.file_type = Some(TryConvert::try_convert(val)?);
    }

    if let Some(val) = options.get(ruby.to_symbol("extension"))
        && !val.is_nil()
    {
        config.extension = Some(TryConvert::try_convert(val)?);
    }

    if let Some(val) = options.get(ruby.to_symbol("exclude"))
        && !val.is_nil()
    {
        let exclude_array: RArray = TryConvert::try_convert(val)?;
        let mut excludes = Vec::with_capacity(exclude_array.len());
        for exclude_val in exclude_array {
            excludes.push(TryConvert::try_convert(exclude_val)?);
        }
        config.exclude = excludes;
    }

    config.min_size = extract_size(ruby, options, "min_size")?;
    config.max_size = extract_size(ruby, options, "max_size")?;

    config.changed_within = extract_time(ruby, options, "changed_within")?;
    config.changed_before = extract_time(ruby, options, "changed_before")?;

    Ok(config)
}

fn fdr_search(ruby: &Ruby, options: RHash) -> Result<RArray, Error> {
    let config = hash_to_config(ruby, options)?;

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

    fdr_module.define_singleton_method("search", function!(fdr_search, 1))?;
    fdr_module.define_singleton_method("entries", function!(fdr_search, 1))?;
    fdr_module.define_singleton_method("scan", function!(fdr_search, 1))?;

    Ok(())
}
