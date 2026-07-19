//! Debugger + "find what accesses" commands. All proxy to the dedicated
//! debugger/access threads (see [`crate::debugger`]).

use crate::debugger::{start_access, start_debugger};
use crate::dto::RegistersDto;
use crate::state::AppState;
use nemclass_sdk::access::AccessBackend;
use nemclass_sdk::debug::{BackendKind, WatchKind, WatchSize};
use parking_lot::Mutex;
use tauri::{AppHandle, State};

fn parse_backend(s: &str) -> Result<BackendKind, String> {
    match s.to_ascii_lowercase().as_str() {
        "ptrace" => Ok(BackendKind::Ptrace),
        "frida" => Ok(BackendKind::Frida),
        _ => Err(format!("unknown debugger backend `{s}`")),
    }
}

fn parse_watch_kind(s: &str) -> Result<WatchKind, String> {
    match s.to_ascii_lowercase().as_str() {
        "execute" | "exec" => Ok(WatchKind::Execute),
        "write" => Ok(WatchKind::Write),
        "readwrite" | "read" | "access" => Ok(WatchKind::ReadWrite),
        _ => Err(format!("unknown watch kind `{s}`")),
    }
}

fn parse_access_backend(s: &str) -> Result<AccessBackend, String> {
    match s.to_ascii_lowercase().as_str() {
        "hardware" | "hw" => Ok(AccessBackend::Hardware),
        "libiht" | "lbr" => Ok(AccessBackend::LibIht),
        "intelpt" | "pt" => Ok(AccessBackend::IntelPt),
        _ => Err(format!("unknown access backend `{s}`")),
    }
}

/// Attaches a debugger of the given backend to the current target.
#[tauri::command]
pub fn debugger_attach(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
    backend: String,
) -> Result<(), String> {
    let backend = parse_backend(&backend)?;
    let pid = state
        .lock()
        .target
        .as_ref()
        .map(|t| t.id())
        .ok_or("not attached to a process")?;
    let handle = start_debugger(app, pid, backend)?;
    state.lock().debugger = Some(handle);
    Ok(())
}

/// Detaches the debugger.
#[tauri::command]
pub fn debugger_detach(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    state.lock().debugger = None;
    Ok(())
}

/// Whether a debugger is attached, and its backend.
#[tauri::command]
pub fn debugger_status(state: State<'_, Mutex<AppState>>) -> Result<Option<String>, String> {
    Ok(state.lock().debugger.as_ref().map(|h| match h.backend {
        BackendKind::Ptrace => "ptrace".to_string(),
        BackendKind::Frida => "frida".to_string(),
    }))
}

fn with_debugger<T>(
    state: &State<'_, Mutex<AppState>>,
    f: impl FnOnce(&crate::debugger::DebuggerHandle) -> Result<T, String>,
) -> Result<T, String> {
    let st = state.lock();
    let h = st.debugger.as_ref().ok_or("no debugger attached")?;
    f(h)
}

/// Lists the target's threads.
#[tauri::command]
pub fn debugger_threads(state: State<'_, Mutex<AppState>>) -> Result<Vec<u32>, String> {
    with_debugger(&state, |h| h.threads())
}

/// Sets a software breakpoint; returns its id.
#[tauri::command]
pub fn bp_set_sw(state: State<'_, Mutex<AppState>>, addr: u64) -> Result<u64, String> {
    with_debugger(&state, |h| h.set_sw_bp(addr as usize))
}

/// Sets a hardware breakpoint/watchpoint; returns its id.
#[tauri::command]
pub fn bp_set_hw(
    state: State<'_, Mutex<AppState>>,
    addr: u64,
    size: usize,
    kind: String,
) -> Result<u64, String> {
    let watch = parse_watch_kind(&kind)?;
    with_debugger(&state, |h| {
        h.set_hw_bp(addr as usize, WatchSize::for_len(size), watch)
    })
}

/// Clears a breakpoint.
#[tauri::command]
pub fn bp_clear(state: State<'_, Mutex<AppState>>, id: u64) -> Result<(), String> {
    with_debugger(&state, |h| h.clear_bp(id))
}

/// Resumes the target.
#[tauri::command]
pub fn dbg_continue(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    with_debugger(&state, |h| h.cont())
}

/// Single-steps a thread.
#[tauri::command]
pub fn dbg_step(state: State<'_, Mutex<AppState>>, tid: u32) -> Result<(), String> {
    with_debugger(&state, |h| h.step(tid))
}

/// Reads a thread's registers.
#[tauri::command]
pub fn dbg_registers(
    state: State<'_, Mutex<AppState>>,
    tid: u32,
) -> Result<RegistersDto, String> {
    with_debugger(&state, |h| h.registers(tid))
}

/// Writes a thread's registers.
#[tauri::command]
pub fn dbg_set_registers(
    state: State<'_, Mutex<AppState>>,
    tid: u32,
    regs: RegistersDto,
) -> Result<(), String> {
    with_debugger(&state, |h| h.set_registers(tid, regs))
}

/// Starts a "find what accesses this address" trace.
#[tauri::command]
pub fn access_start(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
    addr: u64,
    size: usize,
    kind: String,
    backend: String,
) -> Result<(), String> {
    let watch = parse_watch_kind(&kind)?;
    let backend = parse_access_backend(&backend)?;
    let pid = {
        let st = state.lock();
        // The hardware tracer opens its own ptrace attach — can't coexist with a debugger.
        if backend == AccessBackend::Hardware && st.debugger.is_some() {
            return Err("detach the debugger before a hardware access trace".into());
        }
        st.target.as_ref().map(|t| t.id()).ok_or("not attached")?
    };
    let handle = start_access(app, pid, addr as usize, WatchSize::for_len(size), watch, backend)?;
    state.lock().access = Some(handle);
    Ok(())
}

/// Stops the access trace.
#[tauri::command]
pub fn access_stop(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    state.lock().access = None;
    Ok(())
}
