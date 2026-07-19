//! Disassembly, memory map, and lightweight region analysis.

use crate::dto::{InsnDto, MapRegionDto, ModuleInfoDto, StringHitDto};
use crate::state::AppState;
use nemclass_sdk::disasm;
use parking_lot::Mutex;
use tauri::State;

/// The target's `/proc/<pid>/maps`, classified.
#[tauri::command]
pub fn memory_map(state: State<'_, Mutex<AppState>>) -> Result<Vec<MapRegionDto>, String> {
    let pid = state
        .lock()
        .target
        .as_ref()
        .map(|t| t.id())
        .ok_or("not attached")?;
    let regions = disasm::memory_map(pid).map_err(|e| e.to_string())?;
    Ok(regions.iter().map(MapRegionDto::of).collect())
}

/// Loaded modules of the target.
#[tauri::command]
pub fn list_modules(state: State<'_, Mutex<AppState>>) -> Result<Vec<ModuleInfoDto>, String> {
    let target = state.lock().target.clone().ok_or("not attached")?;
    let mods = target.modules().map_err(|e| e.to_string())?;
    Ok(mods
        .into_iter()
        .map(|m| ModuleInfoDto {
            base: m.base as u64,
            size: m.size,
            name: m.name,
        })
        .collect())
}

/// Decodes up to `count` instructions at `start`.
#[tauri::command]
pub fn disassemble(
    state: State<'_, Mutex<AppState>>,
    start: u64,
    count: usize,
) -> Result<Vec<InsnDto>, String> {
    let target = state.lock().target.clone().ok_or("not attached")?;
    let insns = disasm::disassemble(&target, start as usize, count);
    Ok(insns.iter().map(InsnDto::of).collect())
}

/// Extracts printable strings from a range.
#[tauri::command]
pub fn region_strings(
    state: State<'_, Mutex<AppState>>,
    start: u64,
    len: usize,
    min_len: usize,
) -> Result<Vec<StringHitDto>, String> {
    let target = state.lock().target.clone().ok_or("not attached")?;
    let hits = disasm::find_strings(&target, start as usize, len, min_len.max(1));
    Ok(hits
        .into_iter()
        .map(|h| StringHitDto {
            addr: h.addr as u64,
            text: h.text,
        })
        .collect())
}

/// Best-effort function-entry discovery over a range.
#[tauri::command]
pub fn region_functions(
    state: State<'_, Mutex<AppState>>,
    start: u64,
    len: usize,
) -> Result<Vec<u64>, String> {
    let target = state.lock().target.clone().ok_or("not attached")?;
    Ok(disasm::find_functions(&target, start as usize, len)
        .into_iter()
        .map(|a| a as u64)
        .collect())
}

/// Referenced near-call targets over a range.
#[tauri::command]
pub fn region_calls(
    state: State<'_, Mutex<AppState>>,
    start: u64,
    len: usize,
) -> Result<Vec<u64>, String> {
    let target = state.lock().target.clone().ok_or("not attached")?;
    let insns = disasm::disassemble_range(&target, start as usize, len);
    Ok(disasm::call_targets(&insns)
        .into_iter()
        .map(|a| a as u64)
        .collect())
}
