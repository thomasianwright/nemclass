//! The Inspector: walk a class schema over the target's memory and return a tree
//! of live field values; write a single field; raw byte read/write.

use crate::dto::{FieldRow, InspectResult};
use crate::format::{format_bytes, parse_value};
use crate::state::AppState;
use nemclass_sdk::schema::TypeDef;
use nemclass_sdk::types::FieldKind;
use nemclass_sdk::Target;
use parking_lot::Mutex;
use tauri::State;

/// Guards against pointer cycles when recursing into expanded pointers.
const MAX_DEPTH: usize = 8;

#[allow(clippy::too_many_arguments)]
fn walk(
    classes: &[TypeDef],
    class: &str,
    base: u64,
    ptr: usize,
    target: Option<&Target>,
    path: &mut Vec<usize>,
    expanded: &[Vec<usize>],
    depth: usize,
) -> Vec<FieldRow> {
    let Some(t) = classes.iter().find(|c| c.name == class) else {
        return Vec::new();
    };

    let mut rows = Vec::with_capacity(t.fields.len());
    for (i, f) in t.fields.iter().enumerate() {
        let addr = base.wrapping_add(f.offset as u64);
        let size = f.kind.size_with_ptr(ptr);
        let raw = target.and_then(|t| t.read_bytes(addr as usize, size));
        let mut value = raw.as_ref().map(|b| format_bytes(f.kind, b, ptr));
        let mut pointee = None;
        let mut expandable = false;
        let mut children = Vec::new();

        match f.kind {
            FieldKind::Ptr => {
                if let Some(tg) = target {
                    pointee = tg.read_ptr(addr as usize).map(|v| v as u64);
                }
                if let Some(meta) = &f.metadata {
                    if classes.iter().any(|c| &c.name == meta) {
                        expandable = true;
                        path.push(i);
                        if expanded.iter().any(|p| p == path) && depth < MAX_DEPTH {
                            if let Some(pv) = pointee.filter(|&v| v != 0) {
                                children =
                                    walk(classes, meta, pv, ptr, target, path, expanded, depth + 1);
                            }
                        }
                        path.pop();
                    }
                }
            }
            FieldKind::StrPtr => {
                if let Some(tg) = target {
                    if let Some(p) = tg.read_ptr(addr as usize) {
                        pointee = Some(p as u64);
                        if let Some(s) = tg.read_string(p) {
                            value = Some(format!("\"{s}\""));
                        }
                    }
                }
            }
            _ => {}
        }

        rows.push(FieldRow {
            field_index: i,
            offset: f.offset,
            address: addr,
            name: f.name.clone(),
            kind: f.kind.to_kind_string(),
            size,
            value,
            kind_meta: f.metadata.clone(),
            pointee,
            expandable,
            children,
        });
    }
    rows
}

/// Walks `class` at `base`, reading live values in one pass; recurses into any
/// expanded pointer whose path is listed in `expanded`.
#[tauri::command]
pub fn inspect_class(
    state: State<'_, Mutex<AppState>>,
    class: String,
    base: u64,
    expanded: Vec<Vec<usize>>,
) -> Result<InspectResult, String> {
    let (classes, target, ptr) = {
        let st = state.lock();
        (st.classes.clone(), st.target.clone(), st.ptr_size())
    };
    if !classes.iter().any(|c| c.name == class) {
        return Err(format!("no class named `{class}`"));
    }
    let mut path = Vec::new();
    let rows = walk(
        &classes,
        &class,
        base,
        ptr,
        target.as_deref(),
        &mut path,
        &expanded,
        0,
    );
    Ok(InspectResult {
        class_name: class,
        base_addr: base,
        ptr_size: ptr,
        attached: target.is_some(),
        rows,
    })
}

/// Writes a single field value (parsed from text) at an absolute address.
#[tauri::command]
pub fn write_value(
    state: State<'_, Mutex<AppState>>,
    address: u64,
    kind: String,
    text: String,
) -> Result<(), String> {
    let kind = FieldKind::from_kind_string(&kind).ok_or_else(|| format!("unknown kind `{kind}`"))?;
    let (target, ptr) = {
        let st = state.lock();
        (st.target.clone(), st.ptr_size())
    };
    let target = target.ok_or("not attached to a process")?;
    let bytes = parse_value(kind, &text, ptr).ok_or("could not parse value")?;
    if target.write(address as usize, &bytes) {
        Ok(())
    } else {
        Err("memory write failed".into())
    }
}

/// Reads `len` raw bytes at an absolute address.
#[tauri::command]
pub fn read_bytes(
    state: State<'_, Mutex<AppState>>,
    address: u64,
    len: usize,
) -> Result<Vec<u8>, String> {
    let target = state.lock().target.clone().ok_or("not attached")?;
    target
        .read_bytes(address as usize, len)
        .ok_or_else(|| "memory read failed".into())
}

/// Writes raw bytes at an absolute address.
#[tauri::command]
pub fn write_bytes(
    state: State<'_, Mutex<AppState>>,
    address: u64,
    bytes: Vec<u8>,
) -> Result<(), String> {
    let target = state.lock().target.clone().ok_or("not attached")?;
    if target.write(address as usize, &bytes) {
        Ok(())
    } else {
        Err("memory write failed".into())
    }
}
