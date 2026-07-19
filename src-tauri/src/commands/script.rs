//! Lua scripting console. A fresh [`ScriptEngine`] is created per run (Lua state
//! is not `Send`), bridged to the class model through a [`ClassHost`]. An
//! `EXPORT`ed RON project is merged back into the class list.

use crate::dto::ScriptResultDto;
use crate::state::AppState;
use nemclass_sdk::project::{self, SCRIPTS_DIR};
use nemclass_sdk::schema::Project;
use nemclass_scripting::{ClassHost, ScriptEngine, DEFINITIONS};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use tauri::State;

/// A [`ClassHost`] snapshot backed by shared maps, so Lua closures can mutate
/// class addresses that we copy back into [`AppState`] after the run.
#[derive(Clone)]
struct ScriptHost {
    names: Arc<Vec<String>>,
    addrs: Arc<StdMutex<HashMap<String, usize>>>,
}

impl ClassHost for ScriptHost {
    fn class_names(&self) -> Vec<String> {
        (*self.names).clone()
    }
    fn set_class_address(&self, name: &str, address: usize) -> bool {
        if self.names.iter().any(|n| n == name) {
            self.addrs.lock().unwrap().insert(name.to_string(), address);
            true
        } else {
            false
        }
    }
    fn class_address(&self, name: &str) -> Option<usize> {
        self.addrs.lock().unwrap().get(name).copied()
    }
}

/// Runs a Lua console script.
#[tauri::command]
pub fn script_run(
    state: State<'_, Mutex<AppState>>,
    code: String,
) -> Result<ScriptResultDto, String> {
    let (names, addrs, pid) = {
        let st = state.lock();
        (
            st.classes.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
            st.class_addresses.clone(),
            st.target.as_ref().map(|t| t.id()),
        )
    };
    let host = ScriptHost {
        names: Arc::new(names),
        addrs: Arc::new(StdMutex::new(addrs)),
    };

    let engine = ScriptEngine::new().map_err(|e| e.to_string())?;
    let (output, result) = engine.run_console_with_host(&code, pid, host.clone());

    // Copy any script-set class addresses back.
    {
        let mut st = state.lock();
        st.class_addresses = host.addrs.lock().unwrap().clone();
    }

    let (export, error) = match result {
        Ok(exp) => (exp, None),
        Err(e) => (None, Some(e.to_string())),
    };

    // If EXPORT is a RON project, merge it into the class list.
    let mut merged = false;
    if let Some(ron) = &export {
        if let Ok(project) = Project::from_ron(ron) {
            if !project.classes.is_empty() {
                let mut st = state.lock();
                st.snapshot();
                for ty in project.classes {
                    match st.classes.iter().position(|c| c.name == ty.name) {
                        Some(i) => st.classes[i] = ty,
                        None => st.classes.push(ty),
                    }
                }
                merged = true;
            }
        }
    }

    Ok(ScriptResultDto {
        output,
        error,
        export,
        merged,
    })
}

/// Lists `scripts/*.lua` in the open project.
#[tauri::command]
pub fn script_list(state: State<'_, Mutex<AppState>>) -> Result<Vec<String>, String> {
    let dir = state.lock().project_dir.clone().ok_or("no project open")?;
    Ok(project::list_scripts(&dir)
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
        .filter(|n| n != "nem.lua")
        .collect())
}

/// Loads a script's source.
#[tauri::command]
pub fn script_load(
    state: State<'_, Mutex<AppState>>,
    name: String,
) -> Result<String, String> {
    let dir = state.lock().project_dir.clone().ok_or("no project open")?;
    std::fs::read_to_string(dir.join(SCRIPTS_DIR).join(name)).map_err(|e| e.to_string())
}

/// Saves a script and refreshes editor-support definitions.
#[tauri::command]
pub fn script_save(
    state: State<'_, Mutex<AppState>>,
    name: String,
    code: String,
) -> Result<(), String> {
    let dir = state.lock().project_dir.clone().ok_or("no project open")?;
    let name = if name.ends_with(".lua") {
        name
    } else {
        format!("{name}.lua")
    };
    let scripts = dir.join(SCRIPTS_DIR);
    std::fs::create_dir_all(&scripts).map_err(|e| e.to_string())?;
    std::fs::write(scripts.join(&name), code).map_err(|e| e.to_string())?;
    let _ = nemclass_scripting::write_editor_support(&dir);
    Ok(())
}

/// The bundled `nem` LuaCATS definitions (for completions/hover).
#[tauri::command]
pub fn script_definitions() -> Result<String, String> {
    Ok(DEFINITIONS.to_string())
}
