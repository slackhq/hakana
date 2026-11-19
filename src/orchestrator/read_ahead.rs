use std::collections::VecDeque;
use std::fs;
use std::io::Read;
use std::sync::{Condvar, Mutex};
use std::time::SystemTime;

/// Opening and reading a file costs CPU in the kernel, and much more when
/// many threads do it at once (on macOS, eight threads reading webapp's files
/// use 2.5x the CPU one thread does). So a few threads of their own read the
/// files ahead of the threads that parse them, which take them from a
/// [`FileQueue`].
pub(crate) const READERS: usize = 2;

/// How many files may be read and not yet taken.
const CAPACITY: usize = 256;

/// A file read ahead: its index in the list of paths, and its contents with
/// its modification time (`None` if it could not be read here, in which case
/// the thread that takes it reads it itself and reports what went wrong).
pub(crate) struct ReadFile {
    pub index: usize,
    pub contents: Option<(String, u64)>,
}

/// The contents of `path` and its modification time (in microseconds since
/// the epoch), from the same open file. `None` for the HSL and HHI files
/// built into the binary, which are not on disk.
pub(crate) fn read_file(path: &str) -> Option<(String, u64)> {
    if path.starts_with("hsl_embedded_") || path.starts_with("hhi_embedded_") {
        return None;
    }
    let mut file = fs::File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    let updated_time = metadata
        .modified()
        .ok()?
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?
        .as_micros() as u64;
    let mut contents = String::with_capacity(metadata.len() as usize);
    file.read_to_string(&mut contents).ok()?;
    Some((contents, updated_time))
}

/// A bounded queue of files read ahead, between `producers` reading threads
/// and `consumers` parsing threads.
///
/// The readers are faster than the threads that parse, so the queue is mostly
/// full: a reader that finds it full waits until it is half empty, rather than
/// being woken for every file taken, and a side is only signalled when it is
/// waiting. Once every consumer has gone (a [`ConsumerGuard`] is dropped when
/// its thread ends, panicking or not), files are no longer queued, so that the
/// readers finish rather than wait for room that will never come.
pub(crate) struct FileQueue {
    state: Mutex<QueueState>,
    not_empty: Condvar,
    not_full: Condvar,
}

struct QueueState {
    files: VecDeque<ReadFile>,
    producers: usize,
    consumers: usize,
    waiting_producers: usize,
    waiting_consumers: usize,
}

impl FileQueue {
    pub fn new(producers: usize, consumers: usize) -> Self {
        Self {
            state: Mutex::new(QueueState {
                files: VecDeque::with_capacity(CAPACITY),
                producers,
                consumers,
                waiting_producers: 0,
                waiting_consumers: 0,
            }),
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
        }
    }

    /// Queues `file`. `false` if nobody is left to take it: the producer
    /// can stop.
    pub fn push(&self, file: ReadFile) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.files.len() >= CAPACITY {
            state.waiting_producers += 1;
            while state.files.len() > CAPACITY / 2 && state.consumers > 0 {
                state = self.not_full.wait(state).unwrap();
            }
            state.waiting_producers -= 1;
        }
        if state.consumers == 0 {
            return false;
        }
        state.files.push_back(file);
        let wake = state.waiting_consumers > 0;
        drop(state);
        if wake {
            self.not_empty.notify_one();
        }
        true
    }

    /// A producer is done.
    pub fn close(&self) {
        let mut state = self.state.lock().unwrap();
        state.producers -= 1;
        drop(state);
        self.not_empty.notify_all();
    }

    /// The next file, or `None` once every producer is done and the queue is
    /// empty.
    pub fn pop(&self) -> Option<ReadFile> {
        let mut state = self.state.lock().unwrap();
        loop {
            if let Some(file) = state.files.pop_front() {
                let wake = state.waiting_producers > 0 && state.files.len() <= CAPACITY / 2;
                drop(state);
                if wake {
                    self.not_full.notify_all();
                }
                return Some(file);
            }
            if state.producers == 0 {
                return None;
            }
            state.waiting_consumers += 1;
            state = self.not_empty.wait(state).unwrap();
            state.waiting_consumers -= 1;
        }
    }
}

/// Held by a consumer of a [`FileQueue`] for as long as it takes files.
pub(crate) struct ConsumerGuard<'a>(pub &'a FileQueue);

impl Drop for ConsumerGuard<'_> {
    fn drop(&mut self) {
        // (Not `unwrap`: this runs while unwinding too.)
        if let Ok(mut state) = self.0.state.lock() {
            state.consumers -= 1;
            drop(state);
            self.0.not_full.notify_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn every_file_is_taken_once() {
        let queue = Arc::new(FileQueue::new(2, 3));
        let next = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let producers: Vec<_> = (0..2)
            .map(|_| {
                let (queue, next) = (queue.clone(), next.clone());
                std::thread::spawn(move || {
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if index >= 10_000 {
                            break;
                        }
                        assert!(queue.push(ReadFile {
                            index,
                            contents: None
                        }));
                    }
                    queue.close();
                })
            })
            .collect();
        let consumers: Vec<_> = (0..3)
            .map(|_| {
                let queue = queue.clone();
                std::thread::spawn(move || {
                    let _guard = ConsumerGuard(&queue);
                    let mut taken = vec![];
                    while let Some(file) = queue.pop() {
                        taken.push(file.index);
                    }
                    taken
                })
            })
            .collect();
        for producer in producers {
            producer.join().unwrap();
        }
        let mut taken: Vec<usize> = consumers
            .into_iter()
            .flat_map(|c| c.join().unwrap())
            .collect();
        taken.sort();
        assert_eq!(taken, (0..10_000).collect::<Vec<_>>());
    }

    #[test]
    fn producers_stop_when_every_consumer_has_panicked() {
        let queue = Arc::new(FileQueue::new(1, 1));
        let consumer = {
            let queue = queue.clone();
            std::thread::spawn(move || {
                let _guard = ConsumerGuard(&queue);
                queue.pop();
                panic!("consumer failed");
            })
        };
        let mut pushed = 0;
        while queue.push(ReadFile {
            index: pushed,
            contents: None,
        }) {
            pushed += 1;
        }
        queue.close();
        assert!(consumer.join().is_err());
        assert!(pushed <= CAPACITY + 1);
    }
}
