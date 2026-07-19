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
