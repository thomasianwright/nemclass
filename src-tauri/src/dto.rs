//! Serializable data-transfer objects shared with the React frontend.
//!
//! Field naming is `camelCase` on the wire so the TypeScript side reads natural
//! JS keys (`pointerSize`, `fieldCount`, ...).

use nemclass_sdk::schema::{FieldDef, TypeDef};
use nemclass_sdk::types::FieldKind;
use nemclass_sdk::Target;
use serde::{Deserialize, Serialize};

/// A field as supplied by the frontend (e.g. for paste/insert). `kind` is the
/// compact string form; `metadata` is a pointer's target class name.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldInputDto {
    pub name: String,
    pub offset: usize,
    pub kind: String,
    #[serde(default)]
    pub metadata: Option<String>,
}

impl FieldInputDto {
    /// Converts to a schema [`FieldDef`], failing on an unknown kind string.
    pub fn into_field(self) -> Result<FieldDef, String> {
        let kind = FieldKind::from_kind_string(&self.kind)
            .ok_or_else(|| format!("unknown field kind `{}`", self.kind))?;
        Ok(FieldDef {
            name: self.name,
            offset: self.offset,
            kind,
            metadata: self.metadata,
        })
    }
}

/// A running process, for the attach picker.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfoDto {
    pub id: u32,
    pub name: String,
    pub parent_id: u32,
}

/// Summary of the currently attached target.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachedDto {
    pub pid: u32,
    pub name: String,
    pub pointer_size: usize,
    pub is_wine: bool,
    pub is_managed: bool,
}

impl AttachedDto {
    /// Builds a summary from a live target handle.
    pub fn of(t: &Target) -> Self {
        Self {
            pid: t.id(),
            name: t.name(),
            pointer_size: t.pointer_size(),
            is_wine: t.is_wine(),
            is_managed: t.is_managed(),
        }
    }
}

/// One entry in the class list.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassSummaryDto {
    pub name: String,
    pub field_count: usize,
    /// Total layout size in bytes (max field end offset).
    pub size: usize,
}

impl ClassSummaryDto {
    /// Summarizes a class layout, using `ptr_size` to size pointer fields.
    pub fn of(t: &TypeDef, ptr_size: usize) -> Self {
        let size = t
            .fields
            .iter()
            .map(|f| f.offset + f.kind.size_with_ptr(ptr_size))
            .max()
            .unwrap_or(0);
        Self {
            name: t.name.clone(),
            field_count: t.fields.len(),
            size,
        }
    }
}

/// A field kind option for the type-change UI: the compact string plus a label.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindOptionDto {
    /// Compact string form (e.g. `"I32"`, `"Vec3f"`, `"Hex32"`).
    pub kind: String,
    /// Size in bytes at the default pointer width.
    pub size: usize,
}

/// One row in the inspector tree: a field's identity, live value, and (for an
/// expanded pointer) its resolved children.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldRow {
    pub field_index: usize,
    pub offset: usize,
    pub address: u64,
    pub name: String,
    pub kind: String,
    pub size: usize,
    /// Formatted value, or `None` when memory could not be read.
    pub value: Option<String>,
    /// Pointer target class name, if any.
    pub kind_meta: Option<String>,
    /// Resolved pointer value (for `Ptr`/`StrPtr`).
    pub pointee: Option<u64>,
    /// True for a pointer whose target class is known (so it can be expanded).
    pub expandable: bool,
    /// Children of an expanded pointer.
    pub children: Vec<FieldRow>,
}

/// The full inspector view for a class at a base address.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectResult {
    pub class_name: String,
    pub base_addr: u64,
    pub ptr_size: usize,
    pub attached: bool,
    pub rows: Vec<FieldRow>,
}

/// One cheat-table row with its live resolution/value.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheatEntryDto {
    pub index: usize,
    pub description: String,
    pub address: String,
    pub kind: String,
    pub resolved: Option<u64>,
    pub value: Option<String>,
    pub frozen: bool,
}

/// A decoded instruction for the disassembly view.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsnDto {
    pub addr: u64,
    pub len: usize,
    pub bytes: String,
    pub text: String,
    pub kind: String,
    pub target: Option<u64>,
}

impl InsnDto {
    pub fn of(i: &nemclass_sdk::disasm::Insn) -> Self {
        use nemclass_sdk::disasm::FlowKind::*;
        let kind = match i.kind {
            Seq => "seq",
            Call => "call",
            Jump => "jump",
            CondJump => "condJump",
            Ret => "ret",
            Int => "int",
            Bad => "bad",
        };
        let bytes = i
            .bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ");
        Self {
            addr: i.addr as u64,
            len: i.len,
            bytes,
            text: i.text.clone(),
            kind: kind.to_string(),
            target: i.target.map(|t| t as u64),
        }
    }
}

/// A parsed memory-map region.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapRegionDto {
    pub from: u64,
    pub to: u64,
    pub size: usize,
    pub read: bool,
    pub write: bool,
    pub exec: bool,
    pub name: String,
    pub label: String,
    pub kind: String,
}

impl MapRegionDto {
    pub fn of(r: &nemclass_sdk::disasm::MapRegion) -> Self {
        use nemclass_sdk::disasm::RegionKind::*;
        let kind = match r.kind {
            Module => "module",
            Heap => "heap",
            Stack => "stack",
            Vdso => "vdso",
            Anon => "anon",
            Other => "other",
        };
        Self {
            from: r.from as u64,
            to: r.to as u64,
            size: r.size(),
            read: r.read,
            write: r.write,
            exec: r.exec,
            name: r.name.clone(),
            label: r.label(),
            kind: kind.to_string(),
        }
    }
}

/// A string found in memory.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StringHitDto {
    pub addr: u64,
    pub text: String,
}

/// A loaded module.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleInfoDto {
    pub base: u64,
    pub size: usize,
    pub name: String,
}

/// Summary returned after a first/next scan.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummaryDto {
    pub count: usize,
    pub value_type: String,
}

/// One scan result row.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRowDto {
    pub address: u64,
    pub value: Option<String>,
    pub previous: String,
}

/// A scan comparison, from the frontend.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareDto {
    /// One of: exact, unknown, between, greater, less, increased, decreased,
    /// changed, unchanged, increasedBy, decreasedBy.
    pub op: String,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub value2: Option<String>,
}

/// Open-project summary returned after new/open.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStatusDto {
    pub name: String,
    pub dir: Option<String>,
    pub dirty: bool,
    pub class_count: usize,
    pub attached: Option<AttachedDto>,
}
