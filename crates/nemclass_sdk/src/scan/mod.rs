//! CheatEngine-style progressive value scanner.
//!
//! A [`Scanner`] runs over a [`Target`](crate::target::Target) (memory I/O only —
//! no debugger needed). A *first scan* sweeps the target's readable regions for
//! values matching a [`ScanCompare`]; a *next scan* refines a previous
//! [`ScanResults`] set, including relative comparisons (`Increased`, `Changed`,
//! `IncreasedBy`, …) against the snapshot captured last time.
//!
//! ```no_run
//! use nemclass_sdk::{Target, scan::{Scanner, ScanConfig, ScanType, ScanCompare, ScanValue}};
//! let target = Target::attach_pid(1234).unwrap();
//! let scanner = Scanner::new(&target, ScanConfig::new(ScanType::I32));
//! let hits = scanner.first_scan(ScanCompare::Exact(ScanValue::Int(100))).unwrap();
//! // ... player takes damage ...
//! let fewer = scanner.next_scan(&hits, ScanCompare::Decreased).unwrap();
//! ```

use crate::error::{Result, SdkError};
use crate::target::Target;
use crate::types::FieldKind;

mod compare;

/// The interpretation applied to each candidate slot during a scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanType {
    /// Signed 8-bit.
    I8,
    /// Unsigned 8-bit.
    U8,
    /// Signed 16-bit.
    I16,
    /// Unsigned 16-bit.
    U16,
    /// Signed 32-bit.
    I32,
    /// Unsigned 32-bit.
    U32,
    /// Signed 64-bit.
    I64,
    /// Unsigned 64-bit.
    U64,
    /// 32-bit float.
    F32,
    /// 64-bit float.
    F64,
    /// Raw byte sequence (array-of-bytes / AOB). Only [`ScanCompare::Exact`] with
    /// [`ScanValue::Bytes`], `Changed`, and `Unchanged` apply.
    Bytes,
}

impl ScanType {
    /// The fixed slot size in bytes (0 for the variable-length [`ScanType::Bytes`]).
    pub fn size(self) -> usize {
        match self {
            ScanType::I8 | ScanType::U8 => 1,
            ScanType::I16 | ScanType::U16 => 2,
            ScanType::I32 | ScanType::U32 | ScanType::F32 => 4,
            ScanType::I64 | ScanType::U64 | ScanType::F64 => 8,
            ScanType::Bytes => 0,
        }
    }

    /// Whether this is a floating-point type.
    pub fn is_float(self) -> bool {
        matches!(self, ScanType::F32 | ScanType::F64)
    }

    /// The [`FieldKind`] a found address of this type maps to (for turning a scan
    /// hit into a cheat-table / class field). `None` for [`ScanType::Bytes`].
    pub fn to_field_kind(self) -> Option<FieldKind> {
        Some(match self {
            ScanType::I8 => FieldKind::I8,
            ScanType::U8 => FieldKind::U8,
            ScanType::I16 => FieldKind::I16,
            ScanType::U16 => FieldKind::U16,
            ScanType::I32 => FieldKind::I32,
            ScanType::U32 => FieldKind::U32,
            ScanType::I64 => FieldKind::I64,
            ScanType::U64 => FieldKind::U64,
            ScanType::F32 => FieldKind::F32,
            ScanType::F64 => FieldKind::F64,
            ScanType::Bytes => return None,
        })
    }

    /// The scan type corresponding to a scalar [`FieldKind`], if any.
    pub fn from_field_kind(kind: FieldKind) -> Option<ScanType> {
        Some(match kind {
            FieldKind::I8 => ScanType::I8,
            FieldKind::U8 => ScanType::U8,
            FieldKind::I16 => ScanType::I16,
            FieldKind::U16 => ScanType::U16,
            FieldKind::I32 => ScanType::I32,
            FieldKind::U32 => ScanType::U32,
            FieldKind::I64 => ScanType::I64,
            FieldKind::U64 => ScanType::U64,
            FieldKind::F32 => ScanType::F32,
            FieldKind::F64 => ScanType::F64,
            _ => return None,
        })
    }
}

/// A typed value supplied to a comparison.
#[derive(Debug, Clone, PartialEq)]
pub enum ScanValue {
    /// An integer (any width; coerced to the scan type's domain).
    Int(i128),
    /// A float.
    Float(f64),
    /// A raw byte needle (for [`ScanType::Bytes`]).
    Bytes(Vec<u8>),
}

/// How to compare each candidate.
///
/// The relative variants (`Increased`, `Decreased`, `Changed`, `Unchanged`,
/// `IncreasedBy`, `DecreasedBy`) are only valid in [`Scanner::next_scan`]; they
/// compare against the previous [`ScanResults`] snapshot and error on a first scan.
#[derive(Debug, Clone, PartialEq)]
pub enum ScanCompare {
    /// Value equals the given one (float compares are tolerant).
    Exact(ScanValue),
    /// Match every readable slot — the classic "unknown initial value" scan.
    Unknown,
    /// `lo <= value <= hi`.
    Between(ScanValue, ScanValue),
    /// `value > v`.
    Greater(ScanValue),
    /// `value < v`.
    Less(ScanValue),
    /// Value increased since the previous scan.
    Increased,
    /// Value decreased since the previous scan.
    Decreased,
    /// Value changed since the previous scan.
    Changed,
    /// Value did not change since the previous scan.
    Unchanged,
    /// Value increased by exactly this amount.
    IncreasedBy(ScanValue),
    /// Value decreased by exactly this amount.
    DecreasedBy(ScanValue),
}

impl ScanCompare {
    /// Whether this comparison needs a previous snapshot (so it is invalid on a
    /// first scan).
    pub fn is_relative(&self) -> bool {
        matches!(
            self,
            ScanCompare::Increased
                | ScanCompare::Decreased
                | ScanCompare::Changed
                | ScanCompare::Unchanged
                | ScanCompare::IncreasedBy(_)
                | ScanCompare::DecreasedBy(_)
        )
    }
}

/// Scan configuration: value type, address alignment, and region filtering.
#[derive(Debug, Clone)]
pub struct ScanConfig {
    /// The value type to interpret slots as.
    pub value_type: ScanType,
    /// Address alignment for candidate slots (defaults to the type size, min 1).
    pub alignment: usize,
    /// Restrict a first scan to writable regions (game state usually is).
    pub writable_only: bool,
}

impl ScanConfig {
    /// A config for `value_type` with natural alignment and writable-only scanning.
    pub fn new(value_type: ScanType) -> Self {
        Self {
            value_type,
            alignment: value_type.size().max(1),
            writable_only: true,
        }
    }
}

/// The surviving addresses of a scan, plus a snapshot of their values (so a
/// following [`Scanner::next_scan`] can do relative comparisons).
#[derive(Debug, Clone)]
pub struct ScanResults {
    ty: ScanType,
    value_size: usize,
    addrs: Vec<usize>,
    /// `addrs.len() * value_size` bytes: each address's value at scan time.
    snapshot: Vec<u8>,
}

impl ScanResults {
    /// The value type these results were scanned as.
    pub fn value_type(&self) -> ScanType {
        self.ty
    }

    /// The number of surviving addresses.
    pub fn len(&self) -> usize {
        self.addrs.len()
    }

    /// Whether no addresses survived.
    pub fn is_empty(&self) -> bool {
        self.addrs.is_empty()
    }

    /// The surviving addresses.
    pub fn addresses(&self) -> &[usize] {
        &self.addrs
    }

    /// The snapshot value bytes for `addresses()[i]`, or `None` if out of range.
    pub fn value_bytes(&self, i: usize) -> Option<&[u8]> {
        self.snapshot.get(i * self.value_size..(i + 1) * self.value_size)
    }
}

/// A progressive value scanner bound to a target and configuration.
pub struct Scanner<'t> {
    target: &'t Target,
    cfg: ScanConfig,
}

/// Chunk size for streaming a region during a first scan.
const CHUNK: usize = 1 << 16;

impl<'t> Scanner<'t> {
    /// Builds a scanner over `target` with `cfg`.
    pub fn new(target: &'t Target, cfg: ScanConfig) -> Self {
        Self { target, cfg }
    }

    /// The value size for this scan, derived from the type or (for byte scans)
    /// the needle in `cmp`.
    fn value_size(&self, cmp: &ScanCompare) -> Result<usize> {
        if self.cfg.value_type == ScanType::Bytes {
            match cmp {
                ScanCompare::Exact(ScanValue::Bytes(n)) if !n.is_empty() => Ok(n.len()),
                _ => Err(SdkError::Scan(
                    "byte-array scans require Exact(ScanValue::Bytes(..))".into(),
                )),
            }
        } else {
            Ok(self.cfg.value_type.size())
        }
    }

    /// Sweeps the target's readable regions for values matching `cmp`.
    pub fn first_scan(&self, cmp: ScanCompare) -> Result<ScanResults> {
        if cmp.is_relative() {
            return Err(SdkError::Scan(
                "relative comparison requires a previous scan (use next_scan)".into(),
            ));
        }
        let vs = self.value_size(&cmp)?;
        let ty = self.cfg.value_type;
        let align = self.cfg.alignment.max(1);

        let mut addrs = Vec::new();
        let mut snapshot = Vec::new();

        for region in self.target.regions() {
            if !region.read || (self.cfg.writable_only && !region.write) {
                continue;
            }
            let mut base = region.from;
            while base < region.to {
                let want = (CHUNK + vs).min(region.to - base);
                let mut buf = vec![0u8; want];
                if !self.target.read(base, &mut buf) {
                    base += CHUNK;
                    continue;
                }
                // Only accept slots starting in [base, base+CHUNK) so successive
                // windows neither overlap nor gap; the extra `vs` bytes let a slot
                // at the window's end be read whole.
                let first = base.next_multiple_of(align);
                let mut addr = first;
                while addr < base + CHUNK && addr < region.to {
                    let off = addr - base;
                    if off + vs <= buf.len() {
                        let slot = &buf[off..off + vs];
                        if compare::passes(ty, &cmp, None, slot) {
                            addrs.push(addr);
                            snapshot.extend_from_slice(slot);
                        }
                    }
                    addr += align;
                }
                base += CHUNK;
            }
        }

        Ok(ScanResults { ty, value_size: vs, addrs, snapshot })
    }

    /// Refines `prev` by re-reading each surviving address and applying `cmp`
    /// (relative comparisons use `prev`'s snapshot). Addresses that are no longer
    /// readable are dropped.
    pub fn next_scan(&self, prev: &ScanResults, cmp: ScanCompare) -> Result<ScanResults> {
        let ty = prev.ty;
        let vs = prev.value_size;
        if prev.addrs.is_empty() {
            return Ok(prev.clone());
        }

        // Bulk-refresh every candidate in as few syscalls as the backend allows.
        let mut fresh = vec![0u8; prev.addrs.len() * vs];
        {
            let mut regions: Vec<(usize, &mut [u8])> = prev
                .addrs
                .iter()
                .copied()
                .zip(fresh.chunks_mut(vs))
                .collect();
            self.target.read_scatter(&mut regions);
        }

        let mut addrs = Vec::new();
        let mut snapshot = Vec::new();
        for (i, &addr) in prev.addrs.iter().enumerate() {
            let new = &fresh[i * vs..(i + 1) * vs];
            let old = &prev.snapshot[i * vs..(i + 1) * vs];
            // Re-verify readability so freed pages don't survive on stale bytes.
            if !self.target.can_read(addr) {
                continue;
            }
            if compare::passes(ty, &cmp, Some(old), new) {
                addrs.push(addr);
                snapshot.extend_from_slice(new);
            }
        }

        Ok(ScanResults { ty, value_size: vs, addrs, snapshot })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Scans our own process memory for a known heap value, then refines.
    // Relies on same-process `process_vm_readv`; skips cleanly if unavailable.
    #[test]
    fn self_scan_finds_and_refines() {
        let boxed = Box::new(0x1234_5678u32);
        let addr = &*boxed as *const u32 as usize;

        let Ok(target) = Target::attach_pid(std::process::id()) else {
            eprintln!("skip: cannot attach to self (ptrace_scope?)");
            return;
        };

        let scanner = Scanner::new(&target, ScanConfig::new(ScanType::U32));
        let first = scanner
            .first_scan(ScanCompare::Exact(ScanValue::Int(0x1234_5678)))
            .unwrap();
        assert!(
            first.addresses().contains(&addr),
            "expected our value's address among {} hits",
            first.len()
        );

        let same = scanner.next_scan(&first, ScanCompare::Unchanged).unwrap();
        assert!(same.addresses().contains(&addr));

        let none = scanner
            .next_scan(&first, ScanCompare::Exact(ScanValue::Int(0)))
            .unwrap();
        assert!(!none.addresses().contains(&addr));
    }

    #[test]
    fn relative_on_first_scan_errors() {
        let Ok(target) = Target::attach_pid(std::process::id()) else {
            return;
        };
        let scanner = Scanner::new(&target, ScanConfig::new(ScanType::I32));
        assert!(scanner.first_scan(ScanCompare::Increased).is_err());
    }
}
