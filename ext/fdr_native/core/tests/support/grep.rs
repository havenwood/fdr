use fdr_core::{GrepConfig, SearchError, grep_stream};
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct GrepResult {
    pub path: Vec<u8>,
    pub lines: Vec<(u64, Vec<u8>)>,
}

pub fn collect(config: &GrepConfig, cancelled: bool) -> Result<Vec<GrepResult>, SearchError> {
    let grouped = Mutex::new(HashMap::<Vec<u8>, BTreeMap<u64, Vec<u8>>>::new());

    grep_stream(config, &AtomicBool::new(cancelled), |batch| {
        let mut grouped = lock(&grouped);
        for matched in batch {
            let lines = grouped.entry(matched.path.to_vec()).or_default();
            lines
                .entry(matched.line_number)
                .or_insert_with(|| matched.text.to_vec());
        }
        drop(grouped);
        true
    })?;

    Ok(grouped
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .into_iter()
        .map(|(path, lines)| GrepResult {
            path,
            lines: lines.into_iter().collect(),
        })
        .collect())
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
