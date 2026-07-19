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

/// x86-64 register file, both ways over the wire.
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct RegistersDto {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub rip: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub eflags: u64,
}

impl RegistersDto {
    pub fn of(r: &nemclass_sdk::debug::Registers) -> Self {
        Self {
            rax: r.rax,
            rbx: r.rbx,
            rcx: r.rcx,
            rdx: r.rdx,
            rsi: r.rsi,
            rdi: r.rdi,
            rbp: r.rbp,
            rsp: r.rsp,
            rip: r.rip,
            r8: r.r8,
            r9: r.r9,
            r10: r.r10,
            r11: r.r11,
            r12: r.r12,
            r13: r.r13,
            r14: r.r14,
            r15: r.r15,
            eflags: r.eflags,
        }
    }

    /// Applies these values onto an existing register file (segments untouched).
    pub fn apply(&self, r: &mut nemclass_sdk::debug::Registers) {
        r.rax = self.rax;
        r.rbx = self.rbx;
        r.rcx = self.rcx;
        r.rdx = self.rdx;
        r.rsi = self.rsi;
        r.rdi = self.rdi;
        r.rbp = self.rbp;
        r.rsp = self.rsp;
        r.rip = self.rip;
        r.r8 = self.r8;
        r.r9 = self.r9;
        r.r10 = self.r10;
        r.r11 = self.r11;
        r.r12 = self.r12;
        r.r13 = self.r13;
        r.r14 = self.r14;
        r.r15 = self.r15;
        r.eflags = self.eflags;
    }
}

/// A debugger stop event pushed to the frontend.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebugEventDto {
    pub tid: u32,
    pub reason: String,
    pub addr: Option<u64>,
    pub bp_id: Option<u64>,
    pub exit_code: Option<i32>,
}

impl DebugEventDto {
    pub fn of(ev: nemclass_sdk::debug::DebugEvent) -> Self {
        use nemclass_sdk::debug::StopReason::*;
        let (reason, addr, bp_id, exit_code) = match ev.reason {
            Breakpoint { id, addr } => ("breakpoint", Some(addr as u64), Some(id.0), None),
            Watchpoint { id, addr } => ("watchpoint", Some(addr as u64), Some(id.0), None),
            SingleStep => ("singleStep", None, None, None),
            Signal(s) => ("signal", None, None, Some(s)),
            Exited(c) => ("exited", None, None, Some(c)),
            ThreadCreated(_) => ("threadCreated", None, None, None),
            Unknown => ("unknown", None, None, None),
        };
        Self {
            tid: ev.tid.0,
            reason: reason.to_string(),
            addr,
            bp_id,
            exit_code,
        }
    }
}

/// One "what accesses this" record pushed to the frontend.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AccessRecordDto {
    pub insn_addr: u64,
    pub hits: u64,
    pub regs: RegistersDto,
}

impl AccessRecordDto {
    pub fn of(r: &nemclass_sdk::access::AccessRecord) -> Self {
        Self {
            insn_addr: r.insn_addr as u64,
            hits: r.hits,
            regs: RegistersDto::of(&r.regs),
        }
    }
}

/// Spider search progress.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpiderStatusDto {
    pub running: bool,
    pub count: usize,
}

/// One spider result row.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpiderResultDto {
    /// Address expression usable as a cheat-table address.
    pub expr: String,
    pub depth: usize,
    pub address: Option<u64>,
    pub value: Option<String>,
}

/// Result of running a Lua console script.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptResultDto {
    pub output: String,
    pub error: Option<String>,
    pub export: Option<String>,
    /// Whether an `EXPORT` project was parsed and merged into the class list.
    pub merged: bool,
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
