//! Lua scripting console. A fresh [`ScriptEngine`] is created per run (Lua state
//! is not `Send`), bridged to the class model through a [`ClassHost`]. An
//! `EXPORT`ed RON project is merged back into the class list.

use crate::dto::ScriptResultDto;
use crate::state::AppState;
use nemclass_sdk::project::{self, SCRIPTS_DIR};
use nemclass_sdk::schema::{FieldDef, Project, TypeDef};
use nemclass_sdk::types::FieldKind;
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

/// Converts a script-exported class into the contiguous, packed layout the
/// inspector expects: fields sorted by offset, with the gaps between them filled
/// by `Unk` padding. `field_at` records absolute offsets in a *sparse* class;
/// without this, the packed inspector re-packs those offsets away on the first
/// edit (`recompact`), so the placement a script asked for wouldn't stick.
fn normalize_contiguous(mut ty: TypeDef, ptr: usize) -> TypeDef {
    ty.fields.sort_by_key(|f| f.offset);
    let mut out: Vec<FieldDef> = Vec::with_capacity(ty.fields.len());
    let mut cur = 0usize;
    for f in ty.fields {
        // Fill the gap before this field with the largest Unk chunk that fits.
        while cur < f.offset {
            let gap = f.offset - cur;
            let (kind, sz) = if gap >= 8 {
                (FieldKind::Unk64, 8)
            } else if gap >= 4 {
                (FieldKind::Unk32, 4)
            } else if gap >= 2 {
                (FieldKind::Unk16, 2)
            } else {
                (FieldKind::Unk8, 1)
            };
            out.push(FieldDef {
                name: String::new(),
                offset: cur,
                kind,
                metadata: None,
            });
            cur += sz;
        }
        // Keep the field at its declared offset (overlaps, if any, are left as-is).
        cur = cur.max(f.offset + f.kind.size_with_ptr(ptr));
        out.push(f);
    }
    TypeDef {
        name: ty.name,
        fields: out,
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
                let ptr = st.ptr_size();
                for ty in project.classes {
                    // Honor `field_at`'s absolute offsets by padding the class into a
                    // contiguous layout the inspector won't re-pack out from under.
                    let ty = normalize_contiguous(ty, ptr);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_at_offsets_are_padded_contiguous() {
        // Mirrors: sge:field_at("EntitySystem", ptr, 0xC0); field_at("PSystem", ptr, 0x90)
        let ty = TypeDef {
            name: "SGE".into(),
            fields: vec![
                FieldDef { name: "EntitySystem".into(), offset: 0xC0, kind: FieldKind::Ptr, metadata: None },
                FieldDef { name: "PSystem".into(), offset: 0x90, kind: FieldKind::Ptr, metadata: None },
            ],
        };

        let out = normalize_contiguous(ty, 8);

        // The named pointers keep the absolute offsets the script asked for.
        let ps = out.fields.iter().find(|f| f.name == "PSystem").unwrap();
        let es = out.fields.iter().find(|f| f.name == "EntitySystem").unwrap();
        assert_eq!(ps.offset, 0x90);
        assert_eq!(es.offset, 0xC0);

        // The layout is gap-free: every field starts exactly where the last ended.
        let mut cur = 0;
        for f in &out.fields {
            assert_eq!(f.offset, cur, "gap/overlap at {:#x}", f.offset);
            cur += f.kind.size_with_ptr(8);
        }
        assert_eq!(cur, 0xC8);
    }
}
