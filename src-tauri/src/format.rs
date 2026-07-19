//! Headless value formatting/parsing for a [`FieldKind`] over raw bytes — the
//! non-rendering half of what the egui field widgets used to do inline.

use nemclass_sdk::types::{FieldKind, FloatWidth};

/// Reads a little-endian pointer-sized value, zero-extended.
fn read_ptr_val(b: &[u8], ptr: usize) -> u64 {
    let n = ptr.min(8).min(b.len());
    let mut buf = [0u8; 8];
    buf[..n].copy_from_slice(&b[..n]);
    u64::from_le_bytes(buf)
}

/// Formats an f64 with a short, round-trippy representation.
fn fmt_float(v: f64) -> String {
    if v == 0.0 {
        "0".into()
    } else if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

fn fmt_components(b: &[u8], count: usize, width: FloatWidth) -> String {
    let sz = width.size();
    let parts: Vec<String> = (0..count)
        .map(|i| {
            let s = i * sz;
            if s + sz <= b.len() {
                fmt_float(width.read(&b[s..]))
            } else {
                "?".into()
            }
        })
        .collect();
    format!("({})", parts.join(", "))
}

/// Formats `bytes` as a human-readable value for `kind`. Returns `"??"` when
/// there aren't enough bytes.
pub fn format_bytes(kind: FieldKind, b: &[u8], ptr: usize) -> String {
    use FieldKind::*;
    let need = kind.size_with_ptr(ptr);
    if b.len() < need {
        return "??".into();
    }
    match kind {
        I8 => (b[0] as i8).to_string(),
        I16 => i16::from_le_bytes([b[0], b[1]]).to_string(),
        I32 => i32::from_le_bytes(b[..4].try_into().unwrap()).to_string(),
        I64 => i64::from_le_bytes(b[..8].try_into().unwrap()).to_string(),
        U8 => b[0].to_string(),
        U16 => u16::from_le_bytes([b[0], b[1]]).to_string(),
        U32 => u32::from_le_bytes(b[..4].try_into().unwrap()).to_string(),
        U64 => u64::from_le_bytes(b[..8].try_into().unwrap()).to_string(),
        F32 => fmt_float(f32::from_le_bytes(b[..4].try_into().unwrap()) as f64),
        F64 => fmt_float(f64::from_le_bytes(b[..8].try_into().unwrap())),
        Bool => if b[0] != 0 { "true" } else { "false" }.into(),
        Unk8 => format!("0x{:02X}", b[0]),
        Unk16 => format!("0x{:04X}", u16::from_le_bytes([b[0], b[1]])),
        Unk32 => format!("0x{:08X}", u32::from_le_bytes(b[..4].try_into().unwrap())),
        Unk64 => format!("0x{:016X}", u64::from_le_bytes(b[..8].try_into().unwrap())),
        Ptr | StrPtr => format!("0x{:X}", read_ptr_val(b, ptr)),
        Vector { components, width } => fmt_components(b, components as usize, width),
        Matrix { rows, cols, width } => {
            fmt_components(b, rows as usize * cols as usize, width)
        }
    }
}

/// Parses `text` into `kind`'s native-endian byte representation for writing.
/// Integers accept `0x`-prefixed hex; booleans accept `true/false/1/0`.
pub fn parse_value(kind: FieldKind, text: &str, ptr: usize) -> Option<Vec<u8>> {
    use FieldKind::*;
    let s = text.trim();

    fn parse_i(s: &str) -> Option<i128> {
        let s = s.replace('_', "");
        let (neg, body) = s.strip_prefix('-').map_or((false, s.as_str()), |r| (true, r));
        let v = if let Some(h) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
            i128::from_str_radix(h, 16).ok()?
        } else {
            body.parse::<i128>().ok()?
        };
        Some(if neg { -v } else { v })
    }
    fn parse_u(s: &str) -> Option<u128> {
        let s = s.replace('_', "");
        if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            u128::from_str_radix(h, 16).ok()
        } else {
            s.parse::<u128>().ok()
        }
    }

    match kind {
        I8 => Some((parse_i(s)? as i8).to_le_bytes().to_vec()),
        I16 => Some((parse_i(s)? as i16).to_le_bytes().to_vec()),
        I32 => Some((parse_i(s)? as i32).to_le_bytes().to_vec()),
        I64 => Some((parse_i(s)? as i64).to_le_bytes().to_vec()),
        U8 => Some((parse_u(s)? as u8).to_le_bytes().to_vec()),
        U16 => Some((parse_u(s)? as u16).to_le_bytes().to_vec()),
        U32 => Some((parse_u(s)? as u32).to_le_bytes().to_vec()),
        U64 => Some((parse_u(s)? as u64).to_le_bytes().to_vec()),
        Unk8 => Some((parse_u(s)? as u8).to_le_bytes().to_vec()),
        Unk16 => Some((parse_u(s)? as u16).to_le_bytes().to_vec()),
        Unk32 => Some((parse_u(s)? as u32).to_le_bytes().to_vec()),
        Unk64 => Some((parse_u(s)? as u64).to_le_bytes().to_vec()),
        F32 => s.parse::<f32>().ok().map(|v| v.to_le_bytes().to_vec()),
        F64 => s.parse::<f64>().ok().map(|v| v.to_le_bytes().to_vec()),
        Bool => match s.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Some(vec![1]),
            "false" | "0" | "no" => Some(vec![0]),
            _ => None,
        },
        Ptr | StrPtr => {
            let v = parse_u(s)? as u64;
            Some(v.to_le_bytes()[..ptr.min(8)].to_vec())
        }
        Vector { components, width } => parse_components(s, components as usize, width),
        Matrix { rows, cols, width } => {
            parse_components(s, rows as usize * cols as usize, width)
        }
    }
}

fn parse_components(s: &str, count: usize, width: FloatWidth) -> Option<Vec<u8>> {
    let cleaned = s.trim_matches(|c| c == '(' || c == ')' || c == '[' || c == ']');
    let parts: Vec<&str> = cleaned.split(',').map(|p| p.trim()).collect();
    if parts.len() != count {
        return None;
    }
    let mut out = Vec::with_capacity(count * width.size());
    for p in parts {
        out.extend(width.parse_to_ne_bytes(p)?);
    }
    Some(out)
}
