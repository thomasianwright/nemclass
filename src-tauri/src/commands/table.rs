//! Cheat table: typed, freezable, pointer-chain-addressed entries.

use crate::dto::CheatEntryDto;
use crate::format::{format_bytes, parse_value};
use crate::state::AppState;
use nemclass_sdk::table::{CheatEntry, FreezeValue};
use nemclass_sdk::types::FieldKind;
use parking_lot::Mutex;
use tauri::State;

fn parse_kind(s: &str) -> Result<FieldKind, String> {
    FieldKind::from_kind_string(s).ok_or_else(|| format!("unknown kind `{s}`"))
}

/// Lists cheat-table entries with their live resolution/value.
#[tauri::command]
pub fn table_list(state: State<'_, Mutex<AppState>>) -> Result<Vec<CheatEntryDto>, String> {
    let st = state.lock();
    let ptr = st.ptr_size();
    let target = st.target.clone();
    Ok(st
        .cheat_table
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let (resolved, value) = match &target {
                Some(t) => match e.resolve(t) {
                    Ok(addr) => {
                        let v = t
                            .read_bytes(addr, e.kind.size_with_ptr(ptr))
                            .map(|b| format_bytes(e.kind, &b, ptr));
                        (Some(addr as u64), v)
                    }
                    Err(_) => (None, None),
                },
                None => (None, None),
            };
            CheatEntryDto {
                index: i,
                description: e.description.clone(),
                address: e.address.clone(),
                kind: e.kind.to_kind_string(),
                resolved,
                value,
                frozen: e.freeze.is_some(),
            }
        })
        .collect())
}

/// Adds a new entry.
#[tauri::command]
pub fn table_add(
    state: State<'_, Mutex<AppState>>,
    description: String,
    address: String,
    kind: String,
) -> Result<(), String> {
    let kind = parse_kind(&kind)?;
    state
        .lock()
        .cheat_table
        .entries
        .push(CheatEntry::new(description, address, kind));
    Ok(())
}

/// Updates an entry's description/address/kind.
#[tauri::command]
pub fn table_update(
    state: State<'_, Mutex<AppState>>,
    index: usize,
    description: String,
    address: String,
    kind: String,
) -> Result<(), String> {
    let kind = parse_kind(&kind)?;
    let mut st = state.lock();
    let e = st
        .cheat_table
        .entries
        .get_mut(index)
        .ok_or("entry index out of range")?;
    e.description = description;
    e.address = address;
    e.kind = kind;
    st.resync_freezer();
    Ok(())
}

/// Removes an entry.
#[tauri::command]
pub fn table_remove(state: State<'_, Mutex<AppState>>, index: usize) -> Result<(), String> {
    let mut st = state.lock();
    if index >= st.cheat_table.entries.len() {
        return Err("entry index out of range".into());
    }
    st.cheat_table.entries.remove(index);
    st.resync_freezer();
    Ok(())
}

/// Writes a value to an entry's resolved address.
#[tauri::command]
pub fn table_write(
    state: State<'_, Mutex<AppState>>,
    index: usize,
    text: String,
) -> Result<(), String> {
    let (target, ptr, entry) = {
        let st = state.lock();
        (
            st.target.clone(),
            st.ptr_size(),
            st.cheat_table
                .entries
                .get(index)
                .cloned()
                .ok_or("entry index out of range")?,
        )
    };
    let target = target.ok_or("not attached")?;
    let addr = entry.resolve(&target).map_err(|e| e.to_string())?;
    let bytes = parse_value(entry.kind, &text, ptr).ok_or("could not parse value")?;
    if target.write(addr, &bytes) {
        Ok(())
    } else {
        Err("memory write failed".into())
    }
}

/// Freezes/unfreezes an entry. Freezing pins its current value.
#[tauri::command]
pub fn table_freeze(
    state: State<'_, Mutex<AppState>>,
    index: usize,
    on: bool,
) -> Result<(), String> {
    let mut st = state.lock();
    if on {
        let target = st.target.clone().ok_or("not attached")?;
        let ptr = st.ptr_size();
        let entry = st
            .cheat_table
            .entries
            .get(index)
            .cloned()
            .ok_or("entry index out of range")?;
        let addr = entry.resolve(&target).map_err(|e| e.to_string())?;
        let bytes = target
            .read_bytes(addr, entry.kind.size_with_ptr(ptr))
            .ok_or("could not read current value")?;
        st.cheat_table.entries[index].freeze = Some(FreezeValue { bytes });
    } else {
        let e = st
            .cheat_table
            .entries
            .get_mut(index)
            .ok_or("entry index out of range")?;
        e.freeze = None;
    }
    st.resync_freezer();
    Ok(())
}
