//! The headless, serializable type-declaration model.
//!
//! [`Project`] / [`TypeDef`] / [`FieldDef`] are the flat, RON-serialized form of
//! a set of class layouts (the same shape the GUI persists to `.yc` project
//! files). Scripts build these via [`TypeBuilder`]; the GUI converts them to and
//! from its live, editable `Class`/`Field` representation.

use crate::error::{Result, SdkError};
use crate::types::{FieldKind, DEFAULT_PTR_SIZE};
use serde::{Deserialize, Serialize};

/// A single field within a [`TypeDef`]: a name, an absolute offset, a kind, and
/// optional metadata (for [`FieldKind::Ptr`], the target type name).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDef {
    /// Field name.
    pub name: String,
    /// Byte offset from the start of the containing type.
    pub offset: usize,
    /// Field kind.
    pub kind: FieldKind,
    /// Optional metadata (e.g. the referenced type name for a pointer).
    pub metadata: Option<String>,
}

/// A class/struct layout: a name and its fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeDef {
    /// Type name.
    pub name: String,
    /// Fields, in declaration order (not necessarily sorted by offset).
    pub fields: Vec<FieldDef>,
}

/// A collection of type declarations — the unit persisted to a project file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Project {
    /// The declared types.
    pub classes: Vec<TypeDef>,
}

impl Project {
    /// An empty project.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses a project from RON text.
    pub fn from_ron(text: &str) -> Result<Self> {
        ron::from_str(text).map_err(|e| SdkError::Project(e.to_string()))
    }

    /// Serializes this project to RON text.
    pub fn to_ron(&self) -> Result<String> {
        ron::to_string(self).map_err(|e| SdkError::Project(e.to_string()))
    }
}

/// Builds a [`TypeDef`] with running offsets, so callers can append fields
/// without computing offsets by hand. Pointer-sized fields advance by the
/// configured pointer width.
pub struct TypeBuilder {
    name: String,
    fields: Vec<FieldDef>,
    offset: usize,
    ptr_size: usize,
}

impl TypeBuilder {
    /// Starts a new type builder assuming 64-bit pointers.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            fields: Vec::new(),
            offset: 0,
            ptr_size: DEFAULT_PTR_SIZE,
        }
    }

    /// Sets the pointer width (4 or 8) used to advance the running offset for
    /// pointer fields, matching the target's architecture.
    pub fn with_ptr_size(mut self, ptr_size: usize) -> Self {
        self.ptr_size = ptr_size;
        self
    }

    /// Appends a field at the current running offset, advancing by its size.
    pub fn field(&mut self, name: impl Into<String>, kind: FieldKind) -> &mut Self {
        self.field_with_meta(name, kind, None)
    }

    /// Like [`Self::field`], but attaches metadata (e.g. a pointer's target type name).
    pub fn field_with_meta(
        &mut self,
        name: impl Into<String>,
        kind: FieldKind,
        metadata: Option<String>,
    ) -> &mut Self {
        self.fields.push(FieldDef {
            name: name.into(),
            offset: self.offset,
            kind,
            metadata,
        });
        self.offset += kind.size_with_ptr(self.ptr_size);
        self
    }

    /// Places a field at an explicit offset; the running offset advances to just
    /// past it. Use for sparse layouts.
    pub fn field_at(
        &mut self,
        name: impl Into<String>,
        kind: FieldKind,
        offset: usize,
    ) -> &mut Self {
        self.fields.push(FieldDef {
            name: name.into(),
            offset,
            kind,
            metadata: None,
        });
        self.offset = offset + kind.size_with_ptr(self.ptr_size);
        self
    }

    /// Advances the running offset by `bytes` without adding a field (padding).
    pub fn pad(&mut self, bytes: usize) -> &mut Self {
        self.offset += bytes;
        self
    }

    /// Finalizes the builder into a [`TypeDef`].
    pub fn build(self) -> TypeDef {
        TypeDef {
            name: self.name,
            fields: self.fields,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FloatWidth;

    #[test]
    fn builder_running_offsets() {
        let t = {
            let mut b = TypeBuilder::new("Player");
            b.field("health", FieldKind::I32)
                .field("pos", FieldKind::Vector { components: 3, width: FloatWidth::F32 })
                .field("next", FieldKind::Ptr);
            b.build()
        };
        assert_eq!(t.fields[0].offset, 0);
        assert_eq!(t.fields[1].offset, 4);
        assert_eq!(t.fields[2].offset, 16); // 4 + 12
    }

    #[test]
    fn ron_roundtrip() {
        let mut project = Project::new();
        let mut b = TypeBuilder::new("Camera");
        b.field("view", FieldKind::Matrix { rows: 4, cols: 4, width: FloatWidth::F64 });
        project.classes.push(b.build());

        let ron = project.to_ron().unwrap();
        let back = Project::from_ron(&ron).unwrap();
        assert_eq!(project, back);
    }
}
