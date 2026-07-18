use super::{
    BoolField, Field, FloatField, HexField, IntField, MatrixField, PointerField,
    StringPointerField, VectorField,
};

// The type model itself now lives in the headless SDK; the GUI layers its
// editable `Field` implementations on top.
pub use nemclass_sdk::{FieldKind, FloatWidth};

/// GUI extension for [`FieldKind`]: materializes a boxed, editable [`Field`].
pub trait FieldKindExt {
    /// Creates the concrete [`Field`] for this kind, with an optional name.
    fn into_field(self, name: Option<String>) -> Box<dyn Field>;
}

impl FieldKindExt for FieldKind {
    fn into_field(self, name: Option<String>) -> Box<dyn Field> {
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
