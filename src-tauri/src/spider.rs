//! Structure Spider: reverse-engineer pointer chains to a known value.
//!
//! Ported from the egui GUI's `spider/scanner.rs` onto the headless
//! [`Target`]. Parallelised with rayon; the outstanding-task `counter` is
//! incremented at each spawn site (root included) and decremented when a task
//! finishes, so completion detection (`counter == 0`) has no start-up race.

use nemclass_sdk::types::FieldKind;
use nemclass_sdk::Target;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// One discovered pointer path: the parent-pointer offsets followed from the
/// root, then the final offset at which the value sits.
#[derive(Clone)]
pub struct SpiderResult {
    pub parent_offsets: Vec<usize>,
    pub offset: usize,
    pub last_value: f64,
}

/// A running (or finished) spider search.
pub struct SpiderHandle {
    pub results: Arc<Mutex<Vec<SpiderResult>>>,
    pub counter: Arc<AtomicUsize>,
    pub cancel: Arc<AtomicBool>,
    pub root: usize,
    pub kind: FieldKind,
}

impl SpiderHandle {
    /// Whether the search still has outstanding tasks.
    pub fn running(&self) -> bool {
        self.counter.load(Ordering::SeqCst) != 0
    }
}

impl Drop for SpiderHandle {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

/// Reads an 8-byte slot as `kind`, widened to f64 for uniform comparison.
pub fn value_num(kind: FieldKind, b: &[u8]) -> f64 {
    use FieldKind::*;
    match kind {
        I8 => b[0] as i8 as f64,
        U8 => b[0] as f64,
        I16 => i16::from_ne_bytes([b[0], b[1]]) as f64,
        U16 => u16::from_ne_bytes([b[0], b[1]]) as f64,
        I32 => i32::from_ne_bytes(b[..4].try_into().unwrap()) as f64,
        U32 => u32::from_ne_bytes(b[..4].try_into().unwrap()) as f64,
        I64 => i64::from_ne_bytes(b[..8].try_into().unwrap()) as f64,
        U64 => u64::from_ne_bytes(b[..8].try_into().unwrap()) as f64,
        F32 => f32::from_ne_bytes(b[..4].try_into().unwrap()) as f64,
        F64 => f64::from_ne_bytes(b[..8].try_into().unwrap()),
        _ => 0.0,
    }
}

#[derive(Clone)]
struct Opts {
    offsets: Arc<Vec<usize>>,
    struct_size: usize,
    alignment: usize,
    address: usize,
    depth: usize,
    kind: FieldKind,
    needle: f64,
}

/// A single search task. The caller has already incremented `counter` for it.
fn search(
    counter: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
    target: Arc<Target>,
    results: Arc<Mutex<Vec<SpiderResult>>>,
    opts: Opts,
) {
    let run = || {
        if opts.depth == 0 || cancel.load(Ordering::SeqCst) {
            return;
        }
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

        let mut address = start;
        while address < end {
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            let mut buf = [0u8; 8];
            if target.read(address, &mut buf) {
                let ptr = usize::from_ne_bytes(buf);
                // Follow aligned, non-null, non-self pointers into readable memory.
                if address % 8 == 0 && ptr != 0 && ptr != opts.address && target.can_read(ptr) {
                    let offset = address - start;
                    let child = Opts {
                        offsets: Arc::new(opts.offsets.iter().copied().chain([offset]).collect()),
                        address: ptr,
                        depth: opts.depth - 1,
                        ..opts.clone()
                    };
                    counter.fetch_add(1, Ordering::SeqCst);
                    let counter = counter.clone();
                    let cancel = cancel.clone();
                    let target = target.clone();
                    let results = results.clone();
                    rayon::spawn(move || search(counter, cancel, target, results, child));
                }

                if value_num(opts.kind, &buf) == opts.needle {
                    results.lock().unwrap().push(SpiderResult {
                        parent_offsets: opts.offsets.to_vec(),
                        offset: address - start,
                        last_value: opts.needle,
                    });
                }
            }
            address = match address.checked_add(alignment) {
                Some(a) => a,
                None => break,
            };
        }
    };
    run();
    counter.fetch_sub(1, Ordering::SeqCst);
}

/// Kicks off a spider search from `root`, returning its live handle.
#[allow(clippy::too_many_arguments)]
pub fn start_search(
    target: Arc<Target>,
    root: usize,
    struct_size: usize,
    alignment: usize,
    depth: usize,
    kind: FieldKind,
    needle: f64,
) -> SpiderHandle {
    let results = Arc::new(Mutex::new(Vec::new()));
    let counter = Arc::new(AtomicUsize::new(1)); // for the root task
    let cancel = Arc::new(AtomicBool::new(false));
    let opts = Opts {
        offsets: Arc::new(Vec::new()),
        struct_size,
        alignment: alignment.max(1),
        address: root,
        depth,
        kind,
        needle,
    };
    {
        let counter = counter.clone();
        let cancel = cancel.clone();
        let target = target.clone();
        let results = results.clone();
        rayon::spawn(move || search(counter, cancel, target, results, opts));
    }
    SpiderHandle {
        results,
        counter,
        cancel,
        root,
        kind,
    }
}

/// Follows a result's pointer chain from `root` to the final value address.
pub fn resolve(target: &Target, root: usize, r: &SpiderResult) -> Option<usize> {
    let mut address = root;
    let mut buf = [0u8; 8];
    for off in &r.parent_offsets {
        if !target.read(address.saturating_add(*off), &mut buf) {
            return None;
        }
        address = usize::from_ne_bytes(buf);
        if address == 0 {
            return None;
        }
    }
    Some(address.saturating_add(r.offset))
}

/// Builds an `offset::eval` address expression for a result, e.g.
/// `[0x1400+0x10]+0x8`, usable directly as a cheat-table address.
pub fn expr(root: usize, r: &SpiderResult) -> String {
    let mut e = format!("0x{root:X}");
    for off in &r.parent_offsets {
        e = format!("[{e}+0x{off:X}]");
    }
    format!("{e}+0x{:X}", r.offset)
}

/// Re-checks a result against `filter`/`new_value`, updating its snapshot value.
pub fn should_remain(
    target: &Target,
    root: usize,
    r: &mut SpiderResult,
    kind: FieldKind,
    filter: &str,
    new_value: f64,
) -> bool {
    let Some(addr) = resolve(target, root, r) else {
        return false;
    };
    let mut buf = [0u8; 8];
    if !target.read(addr, &mut buf) {
        return false;
    }
    let cur = value_num(kind, &buf);
    let keep = match filter {
        "less" => cur < new_value,
        "lessEq" => cur <= new_value,
        "greater" => cur > new_value,
        "greaterEq" => cur >= new_value,
        "equal" => cur == new_value,
        "notEqual" => cur != new_value,
        "changed" => cur != r.last_value,
        "unchanged" => cur == r.last_value,
        _ => true,
    };
    r.last_value = cur;
    keep
}
