//! Persisted configuration (window layout, recent projects, poll rate).

use crate::state::{AppState, Config};
use parking_lot::Mutex;
use tauri::{AppHandle, Manager, State};

/// The current config.
#[tauri::command]
pub fn get_config(state: State<'_, Mutex<AppState>>) -> Result<Config, String> {
    Ok(state.lock().config.clone())
}

/// Replaces and persists the config.
#[tauri::command]
pub fn set_config(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
    config: Config,
) -> Result<(), String> {
    state.lock().config = config.clone();
    if let Ok(dir) = app.path().app_config_dir() {
        let _ = config.save(&dir);
    }
    Ok(())
}

/// Updates just the saved dockview layout and persists (keeps recents intact).
#[tauri::command]
pub fn set_layout(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
    layout: Option<String>,
) -> Result<(), String> {
    let cfg = {
        let mut st = state.lock();
        st.config.layout = layout;
        st.config.clone()
    };
    if let Ok(dir) = app.path().app_config_dir() {
        let _ = cfg.save(&dir);
    }
    Ok(())
}
