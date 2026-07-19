//! Process enumeration and attach/detach.

use crate::dto::{AttachedDto, ProcessInfoDto};
use crate::state::AppState;
use nemclass_sdk::{target, Target};
use parking_lot::Mutex;
use std::path::PathBuf;
use tauri::State;

/// Lists running processes, sorted by name (case-insensitive) then pid.
#[tauri::command]
pub fn list_processes() -> Result<Vec<ProcessInfoDto>, String> {
    let mut list: Vec<ProcessInfoDto> = target::processes()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|p| ProcessInfoDto {
            id: p.id,
            name: p.name,
            parent_id: p.parent_id,
        })
        .collect();
    list.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    Ok(list)
}

/// Attaches natively by pid.
#[tauri::command]
pub fn attach_pid(state: State<'_, Mutex<AppState>>, pid: u32) -> Result<AttachedDto, String> {
    let target = Target::attach_pid(pid).map_err(|e| e.to_string())?;
    let dto = AttachedDto::of(&target);
    state.lock().set_target(target);
    Ok(dto)
}

/// Attaches to the first process matching `name`.
#[tauri::command]
pub fn attach_name(state: State<'_, Mutex<AppState>>, name: String) -> Result<AttachedDto, String> {
    let target = Target::attach_name(&name).map_err(|e| e.to_string())?;
    let dto = AttachedDto::of(&target);
    state.lock().set_target(target);
    Ok(dto)
}

/// Attaches via a managed plugin library (`yc_*` exports).
#[tauri::command]
pub fn attach_managed(
    state: State<'_, Mutex<AppState>>,
    pid: u32,
    plugin: String,
) -> Result<AttachedDto, String> {
    let target = Target::attach_managed(pid, &PathBuf::from(plugin)).map_err(|e| e.to_string())?;
    let dto = AttachedDto::of(&target);
    state.lock().set_target(target);
    Ok(dto)
}

/// Attaches using the project manifest's auto-attach spec.
#[tauri::command]
pub fn auto_attach(state: State<'_, Mutex<AppState>>) -> Result<AttachedDto, String> {
    let spec = state
        .lock()
        .manifest
        .auto_attach
        .clone()
        .ok_or("no auto-attach configured")?;
    let target = Target::from_auto_attach(&spec).map_err(|e| e.to_string())?;
    let dto = AttachedDto::of(&target);
    state.lock().set_target(target);
    Ok(dto)
}

/// Detaches from the current target.
#[tauri::command]
pub fn detach(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    state.lock().clear_target();
    Ok(())
}

/// Returns the current attachment, if any.
#[tauri::command]
pub fn attach_status(state: State<'_, Mutex<AppState>>) -> Result<Option<AttachedDto>, String> {
    Ok(state.lock().target.as_ref().map(|t| AttachedDto::of(t)))
}
