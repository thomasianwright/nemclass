//! Numeric decoding and comparison predicates behind [`Scanner`](super::Scanner).

use super::{ScanCompare, ScanType, ScanValue};
use std::cmp::Ordering;

/// A value decoded into a common numeric domain for comparison.
#[derive(Clone, Copy)]
pub(super) enum Num {
    /// Signed integer domain (all integer scan types widen to `i128`).
    Int(i128),
    /// Floating-point domain.
    Float(f64),
}

impl Num {
    fn order(self, other: Num) -> Option<Ordering> {
        match (self, other) {
            (Num::Int(a), Num::Int(b)) => Some(a.cmp(&b)),
            (Num::Float(a), Num::Float(b)) => a.partial_cmp(&b),
            // Mixed domains never occur: both sides come from the same ScanType.
            _ => None,
        }
    }

    fn approx_eq(self, other: Num) -> bool {
        match (self, other) {
            (Num::Int(a), Num::Int(b)) => a == b,
            // Tolerant float equality so exact-value scans survive rounding of
            // the user-entered value.
            (Num::Float(a), Num::Float(b)) => (a - b).abs() <= 1e-4 * a.abs().max(1.0),
            _ => false,
        }
    }

}

macro_rules! ne_int {
    ($ty:ty, $b:expr) => {
        <$ty>::from_ne_bytes($b.get(..std::mem::size_of::<$ty>())?.try_into().ok()?) as i128
    };
}

/// Decodes `bytes` as `ty` into the comparison domain. `None` when `bytes` is too
/// short or `ty` is not numeric (e.g. [`ScanType::Bytes`]).
pub(super) fn decode(ty: ScanType, bytes: &[u8]) -> Option<Num> {
    Some(match ty {
        ScanType::I8 => Num::Int(ne_int!(i8, bytes)),
        ScanType::U8 => Num::Int(ne_int!(u8, bytes)),
        ScanType::I16 => Num::Int(ne_int!(i16, bytes)),
        ScanType::U16 => Num::Int(ne_int!(u16, bytes)),
        ScanType::I32 => Num::Int(ne_int!(i32, bytes)),
        ScanType::U32 => Num::Int(ne_int!(u32, bytes)),
        ScanType::I64 => Num::Int(ne_int!(i64, bytes)),
        ScanType::U64 => Num::Int(ne_int!(u64, bytes)),
        ScanType::F32 => Num::Float(f32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?) as f64),
        ScanType::F64 => Num::Float(f64::from_ne_bytes(bytes.get(..8)?.try_into().ok()?)),
        ScanType::Bytes => return None,
    })
}

fn value_num(ty: ScanType, v: &ScanValue) -> Option<Num> {
    Some(match v {
        ScanValue::Int(i) => Num::Int(*i),
        ScanValue::Float(f) => Num::Float(*f),
        // Byte needles aren't part of the numeric domain.
        ScanValue::Bytes(_) => return None,
    })
    .map(|n| coerce(ty, n))
}

/// Coerces a user value into the scan type's domain (int types stay int, float
/// types become float) so `Exact(5)` works whether typed as int or float.
fn coerce(ty: ScanType, n: Num) -> Num {
    match (ty.is_float(), n) {
        (true, Num::Int(i)) => Num::Float(i as f64),
        (false, Num::Float(f)) => Num::Int(f as i128),
        _ => n,
    }
}

/// Evaluates `cmp` for a candidate. `old` is the previous snapshot bytes for the
/// same address (required by the relative comparisons), `new` the freshly-read
/// bytes. Returns whether the candidate survives.
pub(super) fn passes(ty: ScanType, cmp: &ScanCompare, old: Option<&[u8]>, new: &[u8]) -> bool {
    // Byte-array scans compare raw bytes, not numbers.
    if ty == ScanType::Bytes {
        return match cmp {
            ScanCompare::Exact(ScanValue::Bytes(needle)) => new.starts_with(needle),
            ScanCompare::Unchanged => old.is_some_and(|o| o == new),
            ScanCompare::Changed => old.is_some_and(|o| o != new),
            _ => false,
        };
    }

    let Some(n) = decode(ty, new) else { return false };
    let cmp_ord = |a: Num, b: Num, want: &[Ordering]| a.order(b).is_some_and(|o| want.contains(&o));
    let old_num = || old.and_then(|o| decode(ty, o));

    match cmp {
        ScanCompare::Unknown => true,
        ScanCompare::Exact(v) => value_num(ty, v).is_some_and(|t| n.approx_eq(t)),
        ScanCompare::Greater(v) => value_num(ty, v).is_some_and(|t| cmp_ord(n, t, &[Ordering::Greater])),
        ScanCompare::Less(v) => value_num(ty, v).is_some_and(|t| cmp_ord(n, t, &[Ordering::Less])),
        ScanCompare::Between(a, b) => {
            let (Some(a), Some(b)) = (value_num(ty, a), value_num(ty, b)) else { return false };
            cmp_ord(n, a, &[Ordering::Greater, Ordering::Equal])
                && cmp_ord(n, b, &[Ordering::Less, Ordering::Equal])
        }
        ScanCompare::Increased => old_num().is_some_and(|o| cmp_ord(n, o, &[Ordering::Greater])),
        ScanCompare::Decreased => old_num().is_some_and(|o| cmp_ord(n, o, &[Ordering::Less])),
        ScanCompare::Changed => old_num().is_some_and(|o| !n.approx_eq(o)),
        ScanCompare::Unchanged => old_num().is_some_and(|o| n.approx_eq(o)),
        ScanCompare::IncreasedBy(d) => match (old_num(), value_num(ty, d)) {
            (Some(o), Some(d)) => n.approx_eq(add(o, d)),
            _ => false,
        },
        ScanCompare::DecreasedBy(d) => match (old_num(), value_num(ty, d)) {
            (Some(o), Some(d)) => n.approx_eq(add(o, negate(d))),
            _ => false,
        },
    }
}

fn add(a: Num, b: Num) -> Num {
    match (a, b) {
        (Num::Int(x), Num::Int(y)) => Num::Int(x + y),
        (Num::Float(x), Num::Float(y)) => Num::Float(x + y),
        _ => Num::Float(f64::NAN),
    }
}

fn negate(a: Num) -> Num {
    match a {
        Num::Int(x) => Num::Int(-x),
        Num::Float(x) => Num::Float(-x),
    }
}
