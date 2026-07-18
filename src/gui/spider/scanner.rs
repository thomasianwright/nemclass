use super::{bytes_to_value, SearchOptions, SearchResult};
use crate::process::Process;
use parking_lot::{Mutex, RwLock};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU16, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub(crate) struct ScannerState {
    results: Arc<Mutex<Vec<SearchResult>>>,
    counter: Arc<AtomicU16>,
    cancel: Arc<AtomicBool>,
    start: Instant,
    active: bool,
}

pub(crate) enum ScannerReport {
    Finshed(Duration, Vec<SearchResult>),
    InProgress,
    Idle,
}

impl ScannerState {
    pub fn new() -> Self {
        Self {
            counter: Arc::default(),
            results: Arc::default(),
            cancel: Arc::default(),
            start: Instant::now(),
            active: false,
        }
    }

    pub fn active(&self) -> bool {
        self.active
    }

    pub fn begin(&mut self, process: &Arc<RwLock<Option<Process>>>, options: SearchOptions) {
        self.active = true;
        self.start = Instant::now();
        self.counter.store(0, Ordering::SeqCst);
        self.cancel.store(false, Ordering::SeqCst);

        recursive_first_search(
            self.counter.clone(),
            self.cancel.clone(),
            process.clone(),
            self.results.clone(),
            options,
        );
    }

    /// Requests that an in-flight scan stop. Already-queued tasks drain naturally (each still
    /// decrements the counter), so `try_take` reports `Finished` with the partial results.
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn try_take(&mut self) -> ScannerReport {
        if self.active {
            if self.counter.load(Ordering::SeqCst) == 0 {
                self.active = false;
                ScannerReport::Finshed(
                    self.start.elapsed(),
                    std::mem::take(&mut *self.results.lock()),
                )
            } else {
                ScannerReport::InProgress
            }
        } else {
            ScannerReport::Idle
        }
    }
}

fn recursive_first_search(
    counter: Arc<AtomicU16>,
    cancel: Arc<AtomicBool>,
    process: Arc<RwLock<Option<Process>>>,
    results: Arc<Mutex<Vec<SearchResult>>>,
    opts: SearchOptions,
) {
    // Bail before touching the counter so these early returns never leave it unbalanced —
    // an unbalanced counter would hang completion detection (`counter == 0`) forever.
    if opts.depth == 0 || cancel.load(Ordering::SeqCst) {
        return;
    }

    // Compute the aligned, overflow-checked scan range up front.
    let alignment = opts.alignment.max(1);
    let start = match opts.address % alignment {
        0 => opts.address,
        rem => match opts.address.checked_add(alignment - rem) {
            Some(s) => s,
            None => return,
        },
    };
    let Some(end) = start.checked_add(opts.struct_size) else {
        return;
    };

    counter.fetch_add(1, Ordering::SeqCst);

    {
        // Lock the process once for this node. If it was detached, end the node cleanly (the
        // `fetch_sub` below still runs). A read guard is shared, so sibling scan tasks and spawned
        // children never block each other.
        let guard = process.read();
        if let Some(proc) = guard.as_ref() {
            let mut address = start;
            while address < end {
                if cancel.load(Ordering::SeqCst) {
                    break;
                }

                let mut buf = [0u8; 8];
                // Only act on a successful read — a failed read leaves `buf` stale/garbage.
                if proc.read(address, &mut buf[..]) {
                    let target = usize::from_ne_bytes(buf);

                    // Follow only aligned, non-null, non-self pointers into readable memory.
                    if address % 8 == 0
                        && target != 0
                        && target != opts.address
                        && proc.can_read(target)
                    {
                        let offset = address - start;
                        rayon::spawn({
                            let results = results.clone();
                            let offsets = opts.offsets.clone();
                            let process = process.clone();
                            let counter = counter.clone();
                            let cancel = cancel.clone();

                            move || {
                                recursive_first_search(
                                    counter,
                                    cancel,
                                    process,
                                    results,
                                    SearchOptions {
                                        offsets: Arc::new(
                                            offsets.iter().copied().chain([offset]).collect(),
                                        ),
                                        address: target,
                                        struct_size: opts.struct_size,
                                        alignment: opts.alignment,
                                        depth: opts.depth - 1,
                                        value: opts.value,
                                    },
                                );
                            }
                        });
                    }

                    let value = bytes_to_value(&buf, opts.value.kind());
                    if value == opts.value {
                        results.lock().push(SearchResult {
                            parent_offsets: opts.offsets.clone(),
                            offset: address - start,
                            last_value: value,
                        });
                    }
                }

                address = match address.checked_add(alignment) {
                    Some(a) => a,
                    None => break,
                };
            }
        }
    }

    counter.fetch_sub(1, Ordering::SeqCst);
}
