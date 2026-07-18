//! Heuristic type inference: given a window of bytes read from the target process, guess the most
//! likely [`FieldKind`]. Used by the right-click "Guess type" action and the live Hex-view hints.

use super::{FieldKind, FloatWidth};
use crate::process::Process;

/// A `usize` below this is treated as a small integer, not a pointer.
const MIN_POINTER: usize = 0x1_0000;
/// Widest inference window we read/consider (enough for a Vec4 of f32 or a Vec2 of f64).
const WINDOW: usize = 16;

/// Reads up to [`WINDOW`] bytes at `address`, shrinking to 8 if the wider read fails. Returns an
/// empty vec if nothing could be read.
pub fn read_window(process: &Process, address: usize) -> Vec<u8> {
    let mut buf = [0u8; WINDOW];
    if process.read(address, &mut buf) {
        return buf.to_vec();
    }
    let mut small = [0u8; 8];
    if process.read(address, &mut small) {
        return small.to_vec();
    }
    vec![]
}

/// True if the little-endian f32 at the start of `bytes` looks like a real value rather than random
/// bits: normal (no zero/NaN/inf/denormal) and within a sane magnitude band.
fn plausible_f32(bytes: &[u8]) -> bool {
    let Ok(arr) = bytes[..4].try_into() else {
        return false;
    };
    let v = f32::from_ne_bytes(arr);
    v.is_normal() && (1e-6..=1e9).contains(&v.abs())
}

/// f64 counterpart of [`plausible_f32`].
fn plausible_f64(bytes: &[u8]) -> bool {
    let Ok(arr) = bytes[..8].try_into() else {
        return false;
    };
    let v = f64::from_ne_bytes(arr);
    v.is_normal() && (1e-6..=1e9).contains(&v.abs())
}

/// Counts how many consecutive `lane`-byte groups from the start of `bytes` satisfy `plausible`.
fn count_lanes(bytes: &[u8], lane: usize, plausible: impl Fn(&[u8]) -> bool) -> usize {
    let mut n = 0;
    let mut off = 0;
    while off + lane <= bytes.len() && plausible(&bytes[off..off + lane]) {
        n += 1;
        off += lane;
    }
    n
}

/// Does the pointer `addr` target readable, mostly-printable, NUL-terminated ASCII?
fn points_to_string(process: &Process, addr: usize) -> bool {
    let mut buf = [0u8; 16];
    if !process.read(addr, &mut buf) {
        return false;
    }
    let printable = buf
        .iter()
        .take_while(|&&b| b != 0)
        .take(4)
        .filter(|&&b| b.is_ascii_graphic() || b == b' ')
        .count();
    // At least a few printable leading bytes and a NUL somewhere in the window.
    printable >= 3 && buf.contains(&0)
}

/// Returns candidate [`FieldKind`]s for `bytes`, best first. Empty if nothing beats raw bytes.
///
/// Priority: valid pointer (string pointer preferred) > multi-lane float vector (more lanes / f32
/// first) > scalar f64 > scalar f32. Conservative on purpose so live hints stay low-noise.
pub fn infer_kind(bytes: &[u8], process: &Process) -> Vec<FieldKind> {
    let mut out = vec![];

    // Pointer — strongest signal.
    if bytes.len() >= 8 {
        let v = usize::from_ne_bytes(bytes[..8].try_into().unwrap());
        if v >= MIN_POINTER && process.can_read(v) {
            if points_to_string(process, v) {
                out.push(FieldKind::StrPtr);
            }
            out.push(FieldKind::Ptr);
        }
    }

    // Best float vector: prefer the width that yields more consistent lanes, f32 on ties.
    let f32_lanes = count_lanes(bytes, 4, plausible_f32).min(4);
    let f64_lanes = count_lanes(bytes, 8, plausible_f64).min(4);
    let vector = if f32_lanes >= 2 && f32_lanes >= f64_lanes {
        Some(FieldKind::Vector {
            components: f32_lanes as u8,
            width: FloatWidth::F32,
        })
    } else if f64_lanes >= 2 {
        Some(FieldKind::Vector {
            components: f64_lanes as u8,
            width: FloatWidth::F64,
        })
    } else {
        None
    };
    out.extend(vector);

    // Scalar floats.
    if bytes.len() >= 8 && plausible_f64(&bytes[..8]) {
        out.push(FieldKind::F64);
    }
    if bytes.len() >= 4 && plausible_f32(&bytes[..4]) {
        out.push(FieldKind::F32);
    }

    out
}

/// The best inferred kind that is a float or vector (skips pointers, which the Hex view already
/// signals via its pointer column). Used for the low-noise live hints.
pub fn infer_float_hint(bytes: &[u8], process: &Process) -> Option<FieldKind> {
    infer_kind(bytes, process).into_iter().find(|k| {
        matches!(
            k,
            FieldKind::F32 | FieldKind::F64 | FieldKind::Vector { .. }
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{count_lanes, plausible_f32, plausible_f64};

    #[test]
    fn float_plausibility() {
        // Real-looking floats pass.
        assert!(plausible_f32(&1.5f32.to_ne_bytes()));
        assert!(plausible_f32(&(-250.0f32).to_ne_bytes()));
        assert!(plausible_f64(&3.14159f64.to_ne_bytes()));

        // Zero / NaN / out-of-band / small-int bit patterns are rejected.
        assert!(!plausible_f32(&0.0f32.to_ne_bytes()));
        assert!(!plausible_f32(&f32::NAN.to_ne_bytes()));
        assert!(!plausible_f32(&1e12f32.to_ne_bytes()));
        assert!(!plausible_f32(&5u32.to_ne_bytes())); // denormal bit pattern
        assert!(!plausible_f64(&0u64.to_ne_bytes()));
    }

    #[test]
    fn lane_counting_stops_at_first_implausible() {
        let mut bytes = vec![];
        bytes.extend_from_slice(&1.0f32.to_ne_bytes());
        bytes.extend_from_slice(&2.0f32.to_ne_bytes());
        bytes.extend_from_slice(&3.0f32.to_ne_bytes());
        bytes.extend_from_slice(&0.0f32.to_ne_bytes()); // zero breaks the run
        assert_eq!(count_lanes(&bytes, 4, plausible_f32), 3);
    }
}
