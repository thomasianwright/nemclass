use super::{
    BoolField, Field, FloatField, HexField, IntField, MatrixField, PointerField,
    StringPointerField, VectorField,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Element width of a floating point vector/matrix component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FloatWidth {
    F32,
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[rustfmt::skip]
pub enum FieldKind {
    Unk8, Unk16, Unk32, Unk64,
    I8, I16, I32, I64,
    U8, U16, U32, U64,
    F32, F64,
    Ptr,
    StrPtr,
    Bool,
    Vector { components: u8, width: FloatWidth },
    Matrix { rows: u8, cols: u8, width: FloatWidth },
}

impl FieldKind {
    pub const NAMED_VARIANTS: &[(FieldKind, &'static str)] = &[
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
}

impl FieldKind {
    /// Returns size in bytes.
    pub fn size(&self) -> usize {
        match self {
            Self::Unk8 | Self::I8 | Self::U8 | Self::Bool => 1,
            Self::Unk16 | Self::I16 | Self::U16 => 2,
            Self::Unk32 | Self::I32 | Self::U32 | Self::F32 => 4,
            // TODO(ItsEthra): Pointer size is... sigh, different for 32-bit processes
            Self::Unk64 | Self::I64 | Self::U64 | Self::F64 | Self::Ptr | Self::StrPtr => 8,
            Self::Vector { components, width } => *components as usize * width.size(),
            Self::Matrix { rows, cols, width } => {
                *rows as usize * *cols as usize * width.size()
            }
        }
    }

    pub fn into_field(self, name: Option<String>) -> Box<dyn Field> {
        match self {
            Self::Unk8 => Box::new(HexField::<1>::new()),
            Self::Unk16 => Box::new(HexField::<2>::new()),
            Self::Unk32 => Box::new(HexField::<4>::new()),
            Self::Unk64 => Box::new(HexField::<8>::new()),
            Self::I8 => Box::new(IntField::<1>::signed(name.unwrap_or_else(|| "int8".into()))),
            Self::I16 => Box::new(IntField::<2>::signed(
                name.unwrap_or_else(|| "int16".into()),
            )),
            Self::I32 => Box::new(IntField::<4>::signed(
                name.unwrap_or_else(|| "int32".into()),
            )),
            Self::I64 => Box::new(IntField::<8>::signed(
                name.unwrap_or_else(|| "int64".into()),
            )),
            Self::U8 => Box::new(IntField::<1>::unsigned(
                name.unwrap_or_else(|| "uint8".into()),
            )),
            Self::U16 => Box::new(IntField::<2>::unsigned(
                name.unwrap_or_else(|| "uint16".into()),
            )),
            Self::U32 => Box::new(IntField::<4>::unsigned(
                name.unwrap_or_else(|| "uint32".into()),
            )),
            Self::U64 => Box::new(IntField::<8>::unsigned(
                name.unwrap_or_else(|| "uint64".into()),
            )),
            Self::F32 => Box::new(FloatField::<4>::new(name.unwrap_or_else(|| "float".into()))),
            Self::F64 => Box::new(FloatField::<8>::new(
                name.unwrap_or_else(|| "double".into()),
            )),
            Self::Bool => Box::new(BoolField::new(name.unwrap_or_else(|| "boolean".into()))),
            Self::Ptr => Box::new(PointerField::new(name.unwrap_or_else(|| "pointer".into()))),
            Self::StrPtr => Box::new(StringPointerField::new(
                name.unwrap_or_else(|| "str_ptr".into()),
            )),
            Self::Vector { components, width } => Box::new(VectorField::new(
                components,
                width,
                name.unwrap_or_else(|| "vector".into()),
            )),
            Self::Matrix { rows, cols, width } => Box::new(MatrixField::new(
                rows,
                cols,
                width,
                name.unwrap_or_else(|| "matrix".into()),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FieldKind, FloatWidth};

    #[test]
    fn vector_matrix_sizes() {
        assert_eq!(
            FieldKind::Vector {
                components: 3,
                width: FloatWidth::F32
            }
            .size(),
            12
        );
        assert_eq!(
            FieldKind::Vector {
                components: 4,
                width: FloatWidth::F64
            }
            .size(),
            32
        );
        assert_eq!(
            FieldKind::Matrix {
                rows: 4,
                cols: 4,
                width: FloatWidth::F32
            }
            .size(),
            64
        );
        assert_eq!(
            FieldKind::Matrix {
                rows: 3,
                cols: 4,
                width: FloatWidth::F64
            }
            .size(),
            96
        );
    }
}
