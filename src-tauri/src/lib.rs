//! NemClass Tauri backend.
//!
//! A thin command/IPC layer over `nemclass_sdk`: the frontend (React) drives the
//! backend through `#[tauri::command]`s, which hold the canonical app state
//! ([`state::AppState`]) behind a `Mutex` and delegate to the SDK for all real
//! work (process attach, memory read/write, class schema, disasm, debug, ...).

mod commands;
mod dto;
mod format;
mod state;

use parking_lot::Mutex;
use state::AppState;
use tauri::Manager;

/// Builds and runs the Tauri application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(Mutex::new(AppState::new()));
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
            // project
            commands::project::project_new,
            commands::project::project_open,
            commands::project::project_save,
            commands::project::project_save_as,
            commands::project::is_project_dir,
            commands::project::get_manifest,
            commands::project::set_manifest,
            commands::project::project_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
