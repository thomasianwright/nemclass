//! NemClass Tauri backend.
//!
//! A thin command/IPC layer over `nemclass_sdk`: the frontend (React) drives the
//! backend through `#[tauri::command]`s, which hold the canonical app state
//! ([`state::AppState`]) behind a `Mutex` and delegate to the SDK for all real
//! work (process attach, memory read/write, class schema, disasm, debug, ...).

mod commands;
mod debugger;
mod dto;
mod format;
mod spider;
mod state;

use parking_lot::Mutex;
use state::{AppState, Config};
use tauri::Manager;

/// Builds and runs the Tauri application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let mut st = AppState::new();
            if let Ok(dir) = app.path().app_config_dir() {
                st.config = Config::load(&dir);
            }
            app.manage(Mutex::new(st));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // process
            commands::process::list_processes,
            commands::process::attach_pid,
            commands::process::attach_name,
            commands::process::attach_managed,
            commands::process::auto_attach,
            commands::process::detach,
            commands::process::attach_status,
            // classes / fields
            commands::classes::list_classes,
            commands::classes::get_class,
            commands::classes::add_class,
            commands::classes::rename_class,
            commands::classes::delete_class,
            commands::classes::add_field,
            commands::classes::set_field_name,
            commands::classes::set_field_kind,
            commands::classes::set_field_offset,
            commands::classes::delete_field,
            commands::classes::insert_fields,
            commands::classes::undo,
            commands::classes::redo,
            commands::classes::field_kinds,
            // inspector / memory
            commands::inspect::inspect_class,
            commands::inspect::write_value,
            commands::inspect::read_bytes,
            commands::inspect::write_bytes,
            // scanner
            commands::scan::scan_first,
            commands::scan::scan_next,
            commands::scan::scan_reset,
            commands::scan::scan_page,
            commands::scan::scan_add_to_table,
            // spider
            commands::spider::spider_search,
            commands::spider::spider_status,
            commands::spider::spider_page,
            commands::spider::spider_filter,
            commands::spider::spider_cancel,
            commands::spider::spider_add_to_table,
            // cheat table
            commands::table::table_list,
            commands::table::table_add,
            commands::table::table_update,
            commands::table::table_remove,
            commands::table::table_write,
            commands::table::table_freeze,
            // code generation
            commands::generator::generate_code,
            commands::generator::gen_langs,
            // scripting
            commands::script::script_run,
            commands::script::script_list,
            commands::script::script_load,
            commands::script::script_save,
            commands::script::script_definitions,
            // debugger + access tracer
            commands::debugger::debugger_attach,
            commands::debugger::debugger_detach,
            commands::debugger::debugger_status,
            commands::debugger::debugger_threads,
            commands::debugger::bp_set_sw,
            commands::debugger::bp_set_hw,
            commands::debugger::bp_clear,
            commands::debugger::dbg_continue,
            commands::debugger::dbg_step,
            commands::debugger::dbg_registers,
            commands::debugger::dbg_set_registers,
            commands::debugger::access_start,
            commands::debugger::access_stop,
            // disassembly / memory map
            commands::disasm::memory_map,
            commands::disasm::list_modules,
            commands::disasm::disassemble,
            commands::disasm::region_strings,
            commands::disasm::region_functions,
            commands::disasm::region_calls,
            // project
            commands::project::project_new,
            commands::project::project_open,
            commands::project::project_save,
            commands::project::project_save_as,
            commands::project::is_project_dir,
            commands::project::get_manifest,
            commands::project::set_manifest,
            commands::project::project_status,
            // config
            commands::config::get_config,
            commands::config::set_config,
            commands::config::set_layout,
            // filesystem (in-app directory picker)
            commands::fs::list_dir,
            commands::fs::home_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
