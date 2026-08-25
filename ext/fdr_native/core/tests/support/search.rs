use fdr_core::{SearchConfig, SearchError, search_stream};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

pub fn collect(config: &SearchConfig, cancelled: bool) -> Result<Vec<Vec<u8>>, SearchError> {
    let results = Mutex::new(Vec::new());

    search_stream(config, &AtomicBool::new(cancelled), |batch| {
        lock(&results).extend(batch);
        true
    })?;

    let mut results = results
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    results.sort_unstable();
    Ok(results)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
