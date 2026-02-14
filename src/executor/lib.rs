use hakana_logger::Logger;
use indicatif::{ProgressBar, ProgressStyle};
use rustc_hash::FxHashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// Executes work in parallel across a list of items with progress tracking.
///
/// # Type Parameters
/// * `T` - The type of items to process
/// * `R` - The result type produced by processing each item
/// * `F` - The function type that processes items
///
/// # Arguments
/// * `items` - Vector of items to process
/// * `threads` - Number of threads to use for parallel processing
/// * `logger` - Logger for progress display
/// * `process_fn` - Function that processes a single item and returns a result
/// * `external_counter` - Optional external counter for tracking progress across multiple operations
/// * `total_counter` - Optional external counter for setting total items count
///
/// # Returns
/// Vector of results from processing all items
pub fn parallel_execute<T, R, F>(
    items: Vec<T>,
    threads: u8,
    logger: Arc<Logger>,
    process_fn: F,
    external_counter: Option<Arc<AtomicU32>>,
    total_counter: Option<Arc<AtomicU32>>,
) -> Vec<R>
where
    T: Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> R + Send + Sync + Clone + 'static,
{
    if items.is_empty() {
        return vec![];
    }

    // Set total counter if provided
    if let Some(ref counter) = total_counter {
        counter.store(items.len() as u32, Ordering::Relaxed);
    }

    let bar = if logger.show_progress() {
        let pb = ProgressBar::new(items.len() as u64);
        let sty = ProgressStyle::with_template("{bar:40.green/yellow} {pos:>7}/{len:7}").unwrap();
        pb.set_style(sty);
        Some(Arc::new(pb))
    } else {
        None
    };

    // Use external counter if provided
    let counter = external_counter.unwrap_or_else(|| Arc::new(AtomicU32::new(0)));

    let mut group_size = threads as usize;
    let mut item_groups = FxHashMap::default();

    if items.len() < 4 * group_size {
        group_size = 1;
    }

    for (i, item) in items.into_iter().enumerate() {
        let group = i % group_size;
        item_groups.entry(group).or_insert_with(Vec::new).push(item);
    }

    let results = Arc::new(Mutex::new(Vec::new()));
    let mut handles = vec![];

    for (_, item_group) in item_groups {
        let process_fn = process_fn.clone();
        let bar = bar.clone();
        let counter = counter.clone();
        let results = results.clone();

        let handle = std::thread::spawn(move || {
            for item in item_group {
                let result = process_fn(item);
                results.lock().unwrap().push(result);

                let tally = counter.fetch_add(1, Ordering::Relaxed) + 1;
                update_progressbar(tally as u64, bar.clone());
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    if let Some(bar) = &bar {
        bar.finish_and_clear();
    }

    Arc::into_inner(results).unwrap().into_inner().unwrap()
}

fn update_progressbar(percentage: u64, bar: Option<Arc<ProgressBar>>) {
    if let Some(bar) = bar {
        bar.set_position(percentage);
    }
}
