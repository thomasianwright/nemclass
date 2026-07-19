//! Class and field CRUD over the canonical `Vec<TypeDef>` model, with undo/redo.

use crate::dto::{ClassSummaryDto, FieldInputDto, KindOptionDto};
use crate::state::AppState;
use nemclass_sdk::schema::{FieldDef, TypeDef};
use nemclass_sdk::types::{FieldKind, FloatWidth, DEFAULT_PTR_SIZE};
use parking_lot::Mutex;
use tauri::State;

/// The end offset of a class layout (first free byte), used to append fields.
fn end_offset(t: &TypeDef, ptr: usize) -> usize {
    t.fields
        .iter()
        .map(|f| f.offset + f.kind.size_with_ptr(ptr))
        .max()
        .unwrap_or(0)
}

fn find<'a>(st: &'a mut AppState, class: &str) -> Result<&'a mut TypeDef, String> {
    let idx = st
        .class_index(class)
        .ok_or_else(|| format!("no class named `{class}`"))?;
    Ok(&mut st.classes[idx])
}

/// Greedily splits `n` bytes into unnamed Unk padding fields (Unk64/32/16/8, largest
/// first) — the ReClass "raw bytes" that Add/Insert produce. Offsets are placeholders;
/// call [`recompact`] afterwards.
fn padding_fields(mut n: usize) -> Vec<FieldDef> {
    let mut out = Vec::new();
    for (kind, sz) in [
        (FieldKind::Unk64, 8usize),
        (FieldKind::Unk32, 4),
        (FieldKind::Unk16, 2),
        (FieldKind::Unk8, 1),
    ] {
        while n >= sz {
            out.push(FieldDef {
                name: String::new(),
                offset: 0,
                kind,
                metadata: None,
            });
            n -= sz;
        }
    }
    out
}

/// Rewrites every field offset so the layout is contiguous from 0. The egui inspector
/// treats a class as a packed byte sequence with no gaps; every structural edit
/// (add/insert/remove/retype) restores that invariant here.
fn recompact(t: &mut TypeDef, ptr: usize) {
    let mut off = 0;
    for f in &mut t.fields {
        f.offset = off;
        off += f.kind.size_with_ptr(ptr);
    }
}

/// Converts field `index` to `new` the way egui's `change_field_kind` does: shrinking
/// pads the freed tail with Unk bytes; growing "steals" bytes from the following fields
/// (padding any remainder), erroring if there isn't enough room. Preserves the field's
/// name and recompacts offsets. Caller must have snapshotted.
fn retype_impl(
    t: &mut TypeDef,
    index: usize,
    new: FieldKind,
    metadata: Option<String>,
    ptr: usize,
) -> Result<(), String> {
    let old = t.fields.get(index).ok_or("field index out of range")?;
    let old_size = old.kind.size_with_ptr(ptr);
    let new_size = new.size_with_ptr(ptr);
    let new_field = FieldDef {
        name: old.name.clone(),
        offset: 0,
        kind: new,
        metadata,
    };

    if old_size >= new_size {
        // Shrink (or same size): replace in place and pad the bytes we freed.
        t.fields[index] = new_field;
        for (i, f) in padding_fields(old_size - new_size).into_iter().enumerate() {
            t.fields.insert(index + 1 + i, f);
        }
    } else {
        // Grow: consume following fields until we have enough bytes, then pad the rest.
        let (mut steal_size, mut steal_len) = (0usize, 0usize);
        while steal_size < new_size {
            match t.fields.get(index + steal_len) {
                Some(f) => {
                    steal_size += f.kind.size_with_ptr(ptr);
                    steal_len += 1;
                }
                None => break,
            }
        }
        if steal_size < new_size {
            return Err("Not enough space for a new field".into());
        }
        t.fields.drain(index..index + steal_len);
        t.fields.insert(index, new_field);
        for (i, f) in padding_fields(steal_size - new_size).into_iter().enumerate() {
            t.fields.insert(index + 1 + i, f);
        }
    }

    recompact(t, ptr);
    Ok(())
}

/// Lists every class as a summary.
#[tauri::command]
pub fn list_classes(state: State<'_, Mutex<AppState>>) -> Result<Vec<ClassSummaryDto>, String> {
    let st = state.lock();
    let ptr = st.ptr_size();
    Ok(st
        .classes
        .iter()
        .map(|c| ClassSummaryDto::of(c, ptr))
        .collect())
}

/// Returns one full class layout.
#[tauri::command]
pub fn get_class(state: State<'_, Mutex<AppState>>, name: String) -> Result<TypeDef, String> {
    let st = state.lock();
    let idx = st
        .class_index(&name)
        .ok_or_else(|| format!("no class named `{name}`"))?;
    Ok(st.classes[idx].clone())
}

/// Creates a new empty class; fails on a duplicate name.
#[tauri::command]
pub fn add_class(state: State<'_, Mutex<AppState>>, name: String) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("class name cannot be empty".into());
    }
    let mut st = state.lock();
    if st.class_index(&name).is_some() {
        return Err(format!("a class named `{name}` already exists"));
    }
    st.snapshot();
    st.classes.push(TypeDef {
        name,
        fields: Vec::new(),
    });
    Ok(())
}

/// Renames a class (and any pointer fields that referenced it).
#[tauri::command]
pub fn rename_class(
    state: State<'_, Mutex<AppState>>,
    old: String,
    new: String,
) -> Result<(), String> {
    let new = new.trim().to_string();
    if new.is_empty() {
        return Err("class name cannot be empty".into());
    }
    let mut st = state.lock();
    if st.class_index(&old).is_none() {
        return Err(format!("no class named `{old}`"));
    }
    if old != new && st.class_index(&new).is_some() {
        return Err(format!("a class named `{new}` already exists"));
    }
    st.snapshot();
    for class in &mut st.classes {
        if class.name == old {
            class.name = new.clone();
        }
        for f in &mut class.fields {
            if f.metadata.as_deref() == Some(old.as_str()) {
                f.metadata = Some(new.clone());
            }
        }
    }
    Ok(())
}

/// Deletes a class.
#[tauri::command]
pub fn delete_class(state: State<'_, Mutex<AppState>>, name: String) -> Result<(), String> {
    let mut st = state.lock();
    let idx = st
        .class_index(&name)
        .ok_or_else(|| format!("no class named `{name}`"))?;
    st.snapshot();
    st.classes.remove(idx);
    Ok(())
}

/// Appends a field at the end of a class layout.
#[tauri::command]
pub fn add_field(
    state: State<'_, Mutex<AppState>>,
    class: String,
    name: String,
    kind: String,
    metadata: Option<String>,
) -> Result<(), String> {
    let kind = FieldKind::from_kind_string(&kind).ok_or_else(|| format!("unknown kind `{kind}`"))?;
    let mut st = state.lock();
    let ptr = st.ptr_size();
    st.snapshot();
    let t = find(&mut st, &class)?;
    let offset = end_offset(t, ptr);
    t.fields.push(FieldDef {
        name,
        offset,
        kind,
        metadata,
    });
    Ok(())
}

/// Renames a field by index.
#[tauri::command]
pub fn set_field_name(
    state: State<'_, Mutex<AppState>>,
    class: String,
    index: usize,
    name: String,
) -> Result<(), String> {
    let mut st = state.lock();
    st.snapshot();
    let t = find(&mut st, &class)?;
    let f = t.fields.get_mut(index).ok_or("field index out of range")?;
    f.name = name;
    Ok(())
}

/// Changes a field's kind (and optional pointer-target metadata) by index.
#[tauri::command]
pub fn set_field_kind(
    state: State<'_, Mutex<AppState>>,
    class: String,
    index: usize,
    kind: String,
    metadata: Option<String>,
) -> Result<(), String> {
    let kind = FieldKind::from_kind_string(&kind).ok_or_else(|| format!("unknown kind `{kind}`"))?;
    let mut st = state.lock();
    st.snapshot();
    let t = find(&mut st, &class)?;
    let f = t.fields.get_mut(index).ok_or("field index out of range")?;
    f.kind = kind;
    f.metadata = metadata;
    Ok(())
}

/// Sets a field's absolute offset by index.
#[tauri::command]
pub fn set_field_offset(
    state: State<'_, Mutex<AppState>>,
    class: String,
    index: usize,
    offset: usize,
) -> Result<(), String> {
    let mut st = state.lock();
    st.snapshot();
    let t = find(&mut st, &class)?;
    let f = t.fields.get_mut(index).ok_or("field index out of range")?;
    f.offset = offset;
    Ok(())
}

/// Deletes a field by index.
#[tauri::command]
pub fn delete_field(
    state: State<'_, Mutex<AppState>>,
    class: String,
    index: usize,
) -> Result<(), String> {
    let mut st = state.lock();
    st.snapshot();
    let t = find(&mut st, &class)?;
    if index >= t.fields.len() {
        return Err("field index out of range".into());
    }
    t.fields.remove(index);
    Ok(())
}

/// Inserts a batch of fields (e.g. from a paste) at `at_index`.
#[tauri::command]
pub fn insert_fields(
    state: State<'_, Mutex<AppState>>,
    class: String,
    at_index: usize,
    fields: Vec<FieldInputDto>,
) -> Result<(), String> {
    let parsed: Vec<FieldDef> = fields
        .into_iter()
        .map(|f| f.into_field())
        .collect::<Result<_, _>>()?;
    let mut st = state.lock();
    st.snapshot();
    let t = find(&mut st, &class)?;
    let at = at_index.min(t.fields.len());
    for (i, f) in parsed.into_iter().enumerate() {
        t.fields.insert(at + i, f);
    }
    Ok(())
}

/// Appends `n` bytes of Unk padding to the end of a class (ReClass "Add N bytes").
#[tauri::command]
pub fn add_bytes(state: State<'_, Mutex<AppState>>, class: String, n: usize) -> Result<(), String> {
    if n == 0 {
        return Ok(());
    }
    let mut st = state.lock();
    let ptr = st.ptr_size();
    st.snapshot();
    let t = find(&mut st, &class)?;
    t.fields.extend(padding_fields(n));
    recompact(t, ptr);
    Ok(())
}

/// Inserts `n` bytes of Unk padding before field `index` (ReClass "Insert N bytes").
#[tauri::command]
pub fn insert_bytes(
    state: State<'_, Mutex<AppState>>,
    class: String,
    index: usize,
    n: usize,
) -> Result<(), String> {
    if n == 0 {
        return Ok(());
    }
    let mut st = state.lock();
    let ptr = st.ptr_size();
    st.snapshot();
    let t = find(&mut st, &class)?;
    let at = index.min(t.fields.len());
    for (i, f) in padding_fields(n).into_iter().enumerate() {
        t.fields.insert(at + i, f);
    }
    recompact(t, ptr);
    Ok(())
}

/// Removes `n` fields starting at `index` (ReClass "Remove N fields").
#[tauri::command]
pub fn remove_fields(
    state: State<'_, Mutex<AppState>>,
    class: String,
    index: usize,
    n: usize,
) -> Result<(), String> {
    let mut st = state.lock();
    let ptr = st.ptr_size();
    st.snapshot();
    let t = find(&mut st, &class)?;
    if index >= t.fields.len() {
        return Err("field index out of range".into());
    }
    let end = index.saturating_add(n).min(t.fields.len());
    t.fields.drain(index..end);
    recompact(t, ptr);
    Ok(())
}

/// Converts field `index` to `kind` with ReClass steal/pad resizing (see [`retype_impl`]).
/// This is the toolbar type-change; unlike [`set_field_kind`] it keeps the layout packed.
#[tauri::command]
pub fn retype_field(
    state: State<'_, Mutex<AppState>>,
    class: String,
    index: usize,
    kind: String,
    metadata: Option<String>,
) -> Result<(), String> {
    let new = FieldKind::from_kind_string(&kind).ok_or_else(|| format!("unknown kind `{kind}`"))?;
    let mut st = state.lock();
    let ptr = st.ptr_size();
    st.snapshot();
    let t = find(&mut st, &class)?;
    retype_impl(t, index, new, metadata, ptr)
}

/// Infers a more specific type for the field at `address` and applies it (ReClass
/// "Guess type"). Returns the new kind string, or `None` when nothing beats raw bytes
/// or no process is attached.
#[tauri::command]
pub fn guess_type(
    state: State<'_, Mutex<AppState>>,
    class: String,
    index: usize,
    address: u64,
) -> Result<Option<String>, String> {
    let mut st = state.lock();
    let ptr = st.ptr_size();
    let Some(target) = st.target.clone() else {
        return Ok(None);
    };
    let bytes = nemclass_sdk::infer::read_window(&target, address as usize);
    let Some(kind) = nemclass_sdk::infer::infer_kind(&bytes, &target)
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    let kind_str = kind.to_kind_string();
    st.snapshot();
    let t = find(&mut st, &class)?;
    retype_impl(t, index, kind, None, ptr)?;
    Ok(Some(kind_str))
}

/// The stored base address of a class (0 if unset). This is the single source
/// of truth for a class's base — the Inspector and Lua scripts share it.
#[tauri::command]
pub fn get_class_address(state: State<'_, Mutex<AppState>>, name: String) -> Result<u64, String> {
    Ok(state.lock().class_addresses.get(&name).copied().unwrap_or(0) as u64)
}

/// Sets a class's base address (e.g. from the Inspector's base field).
#[tauri::command]
pub fn set_class_address(
    state: State<'_, Mutex<AppState>>,
    name: String,
    addr: u64,
) -> Result<(), String> {
    state.lock().class_addresses.insert(name, addr as usize);
    Ok(())
}

/// Undo the last class-model change; returns whether anything changed.
#[tauri::command]
pub fn undo(state: State<'_, Mutex<AppState>>) -> Result<bool, String> {
    Ok(state.lock().undo())
}

/// Redo the last undone change; returns whether anything changed.
#[tauri::command]
pub fn redo(state: State<'_, Mutex<AppState>>) -> Result<bool, String> {
    Ok(state.lock().redo())
}

/// The palette of field kinds offered by the type-change UI.
#[tauri::command]
pub fn field_kinds() -> Result<Vec<KindOptionDto>, String> {
    let mut kinds: Vec<FieldKind> = vec![
        FieldKind::Unk8,
        FieldKind::Unk16,
        FieldKind::Unk32,
        FieldKind::Unk64,
        FieldKind::I8,
        FieldKind::I16,
        FieldKind::I32,
        FieldKind::I64,
        FieldKind::U8,
        FieldKind::U16,
        FieldKind::U32,
        FieldKind::U64,
        FieldKind::F32,
        FieldKind::F64,
        FieldKind::Bool,
        FieldKind::Ptr,
        FieldKind::StrPtr,
    ];
    for (c, w) in [
        (2, FloatWidth::F32),
        (3, FloatWidth::F32),
        (4, FloatWidth::F32),
        (2, FloatWidth::F64),
        (3, FloatWidth::F64),
        (4, FloatWidth::F64),
    ] {
        kinds.push(FieldKind::Vector {
            components: c,
            width: w,
        });
    }
    kinds.push(FieldKind::Matrix {
        rows: 4,
        cols: 4,
        width: FloatWidth::F32,
    });

    Ok(kinds
        .into_iter()
        .map(|k| KindOptionDto {
            kind: k.to_kind_string(),
            size: k.size_with_ptr(DEFAULT_PTR_SIZE),
        })
        .collect())
}
