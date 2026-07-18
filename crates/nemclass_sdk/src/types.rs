//! The headless type model: [`FieldKind`] and [`FloatWidth`].
//!
//! This is the pure-data half of the GUI's field system — no rendering, no
//! boxed `Field` trait objects. The GUI re-exports these and layers its
//! editable `Field` implementations on top.

use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Default pointer width, in bytes, assumed when a target's real width is
/// unknown (e.g. code generation, or a managed plugin that hides arch).
pub const DEFAULT_PTR_SIZE: usize = 8;

/// Element width of a floating point vector/matrix component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FloatWidth {
    /// 32-bit float.
    F32,
    /// 64-bit float.
    F64,
}

impl FloatWidth {
    /// Size of a single element in bytes.
    pub fn size(self) -> usize {
        match self {
            Self::F32 => 4,
            Self::F64 => 8,
        }
    }

    /// Short suffix used in display names: `f` for f32, `d` for f64.
    pub fn suffix(self) -> char {
        match self {
            Self::F32 => 'f',
            Self::F64 => 'd',
        }
    }

    /// Rust element type, e.g. `f32`.
    pub fn rust_ty(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }

    /// C++ element type, e.g. `float`.
    pub fn cpp_ty(self) -> &'static str {
        match self {
            Self::F32 => "float",
            Self::F64 => "double",
        }
    }

    /// Reads a single component from the start of `bytes`, widening f32 to f64 for display.
    /// `bytes` must be at least [`Self::size`] long.
    pub fn read(self, bytes: &[u8]) -> f64 {
        match self {
            Self::F32 => f32::from_ne_bytes(bytes[..4].try_into().unwrap()) as f64,
            Self::F64 => f64::from_ne_bytes(bytes[..8].try_into().unwrap()),
        }
    }

    /// Parses a component from text into its native-endian byte representation.
    pub fn parse_to_ne_bytes(self, s: &str) -> Option<Vec<u8>> {
        match self {
            Self::F32 => s.parse::<f32>().ok().map(|v| v.to_ne_bytes().to_vec()),
            Self::F64 => s.parse::<f64>().ok().map(|v| v.to_ne_bytes().to_vec()),
        }
    }
}

/// The kind of a field: its interpretation and byte width.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[rustfmt::skip]
#[allow(missing_docs)]
pub enum FieldKind {
    Unk8, Unk16, Unk32, Unk64,
    I8, I16, I32, I64,
    U8, U16, U32, U64,
    F32, F64,
    Ptr,
    StrPtr,
    Bool,
    /// N-component float vector.
    Vector { components: u8, width: FloatWidth },
    /// `rows`×`cols` float matrix.
    Matrix { rows: u8, cols: u8, width: FloatWidth },
}

impl FieldKind {
    /// The named scalar variants, paired with their canonical labels.
    pub const NAMED_VARIANTS: &'static [(FieldKind, &'static str)] = &[
        (Self::I8, "I8"),
        (Self::I16, "I16"),
        (Self::I32, "I32"),
        (Self::I64, "I64"),
        (Self::U8, "U8"),
        (Self::U16, "U16"),
        (Self::U32, "U32"),
        (Self::U64, "U64"),
        (Self::F32, "F32"),
        (Self::F64, "F64"),
    ];

    /// Canonical label for the named scalar variants, or `None` for the rest.
    pub fn label(&self) -> Option<&'static str> {
        Self::NAMED_VARIANTS
            .iter()
            .find_map(|(v, s)| if v == self { Some(*s) } else { None })
    }

    /// Human-readable name for any kind, including the data-carrying vector/matrix variants
    /// (e.g. `Vec3f`, `Mat4x4d`). Unlike [`Self::label`], this never returns `None`.
    pub fn display_name(&self) -> Cow<'static, str> {
        match self {
            Self::Vector { components, width } => {
                Cow::Owned(format!("Vec{}{}", components, width.suffix()))
            }
            Self::Matrix { rows, cols, width } => {
                Cow::Owned(format!("Mat{}x{}{}", rows, cols, width.suffix()))
            }
            other => Cow::Borrowed(match other {
                Self::Unk8 => "Hex8",
                Self::Unk16 => "Hex16",
                Self::Unk32 => "Hex32",
                Self::Unk64 => "Hex64",
                Self::Ptr => "Ptr",
                Self::StrPtr => "StrPtr",
                Self::Bool => "Bool",
                _ => other.label().unwrap_or("?"),
            }),
        }
    }

    /// The compact string form used in TOML class files (e.g. `"I32"`, `"Vec3f"`,
    /// `"Mat4x4d"`, `"Hex32"`, `"Ptr"`). This is exactly [`Self::display_name`].
    pub fn to_kind_string(&self) -> String {
        self.display_name().into_owned()
    }

    /// Parses the compact string form produced by [`Self::to_kind_string`].
    pub fn from_kind_string(s: &str) -> Option<Self> {
        use FieldKind::*;
        Some(match s {
            "I8" => I8,
            "I16" => I16,
            "I32" => I32,
            "I64" => I64,
            "U8" => U8,
            "U16" => U16,
            "U32" => U32,
            "U64" => U64,
            "F32" => F32,
            "F64" => F64,
            "Bool" => Bool,
            "Ptr" => Ptr,
            "StrPtr" => StrPtr,
            "Hex8" => Unk8,
            "Hex16" => Unk16,
            "Hex32" => Unk32,
            "Hex64" => Unk64,
            other => return parse_vec_mat(other),
        })
    }

    /// Size in bytes, assuming [`DEFAULT_PTR_SIZE`] for pointer kinds. Use
    /// [`Self::size_with_ptr`] when a target's real pointer width is known.
    pub fn size(&self) -> usize {
        self.size_with_ptr(DEFAULT_PTR_SIZE)
    }

    /// Size in bytes, using `ptr_size` (4 or 8) for [`FieldKind::Ptr`] and
    /// [`FieldKind::StrPtr`]. This is what makes 32-bit / WoW64 layouts correct.
    pub fn size_with_ptr(&self, ptr_size: usize) -> usize {
        match self {
            Self::Unk8 | Self::I8 | Self::U8 | Self::Bool => 1,
            Self::Unk16 | Self::I16 | Self::U16 => 2,
            Self::Unk32 | Self::I32 | Self::U32 | Self::F32 => 4,
            Self::Unk64 | Self::I64 | Self::U64 | Self::F64 => 8,
            Self::Ptr | Self::StrPtr => ptr_size,
            Self::Vector { components, width } => *components as usize * width.size(),
            Self::Matrix { rows, cols, width } => *rows as usize * *cols as usize * width.size(),
        }
    }
}

/// Splits a `Vec`/`Mat` suffix into its dimensions text and float width, e.g.
/// `"3f"` -> `("3", F32)`, `"4x4d"` -> `("4x4", F64)`.
fn split_width(s: &str) -> Option<(&str, FloatWidth)> {
    let width = match s.chars().last()? {
        'f' => FloatWidth::F32,
        'd' => FloatWidth::F64,
        _ => return None,
    };
    Some((&s[..s.len() - 1], width))
}

/// Parses the vector/matrix kind strings: `Vec{N}{f|d}` and `Mat{R}x{C}{f|d}`.
fn parse_vec_mat(s: &str) -> Option<FieldKind> {
    if let Some(rest) = s.strip_prefix("Vec") {
        let (num, width) = split_width(rest)?;
        return Some(FieldKind::Vector {
            components: num.parse().ok()?,
            width,
        });
    }
    if let Some(rest) = s.strip_prefix("Mat") {
        let (dims, width) = split_width(rest)?;
        let (rows, cols) = dims.split_once('x')?;
        return Some(FieldKind::Matrix {
            rows: rows.parse().ok()?,
            cols: cols.parse().ok()?,
            width,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{FieldKind, FloatWidth};

    #[test]
    fn kind_string_roundtrip() {
        let kinds = [
            FieldKind::I32,
            FieldKind::U8,
            FieldKind::F64,
            FieldKind::Bool,
            FieldKind::Ptr,
            FieldKind::StrPtr,
            FieldKind::Unk32,
            FieldKind::Vector { components: 3, width: FloatWidth::F32 },
            FieldKind::Vector { components: 4, width: FloatWidth::F64 },
            FieldKind::Matrix { rows: 4, cols: 4, width: FloatWidth::F32 },
            FieldKind::Matrix { rows: 3, cols: 4, width: FloatWidth::F64 },
        ];
        for k in kinds {
            let s = k.to_kind_string();
            assert_eq!(FieldKind::from_kind_string(&s), Some(k), "roundtrip failed for {s}");
        }
        assert_eq!(FieldKind::from_kind_string("Nope"), None);
        assert_eq!(FieldKind::from_kind_string("Vec9q"), None);
    }

    #[test]
    fn vector_matrix_sizes() {
        assert_eq!(
            FieldKind::Vector { components: 3, width: FloatWidth::F32 }.size(),
            12
        );
        assert_eq!(
            FieldKind::Matrix { rows: 4, cols: 4, width: FloatWidth::F32 }.size(),
            64
        );
    }

    #[test]
    fn pointer_width_affects_size() {
        assert_eq!(FieldKind::Ptr.size_with_ptr(8), 8);
        assert_eq!(FieldKind::Ptr.size_with_ptr(4), 4);
        assert_eq!(FieldKind::StrPtr.size_with_ptr(4), 4);
        // Non-pointer kinds are unaffected by pointer width.
        assert_eq!(FieldKind::I32.size_with_ptr(4), 4);
    }
}
