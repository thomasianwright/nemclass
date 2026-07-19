//! Code generation from the class schema.

use crate::state::AppState;
use nemclass_sdk::generator::{generate, Lang};
use nemclass_sdk::schema::Project;
use parking_lot::Mutex;
use tauri::State;

/// Generates source for all classes in the given language (`"Rust"` / `"C++"`).
#[tauri::command]
pub fn generate_code(state: State<'_, Mutex<AppState>>, lang: String) -> Result<String, String> {
    let lang = Lang::parse(&lang).ok_or_else(|| format!("unknown language `{lang}`"))?;
    let project = Project::from_types(state.lock().classes.clone());
    Ok(generate(&project, lang))
}

/// The available code-generation languages.
#[tauri::command]
pub fn gen_langs() -> Result<Vec<String>, String> {
    Ok(Lang::ALL.iter().map(|l| l.label().to_string()).collect())
}
