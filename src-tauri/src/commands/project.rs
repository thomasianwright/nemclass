//! Folder-based project new / open / save + manifest settings.

use crate::dto::{AttachedDto, ProjectStatusDto};
use crate::state::AppState;
use nemclass_sdk::project::{self, Manifest};
use nemclass_sdk::schema::Project;
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use tauri::State;

fn status(st: &AppState) -> ProjectStatusDto {
    ProjectStatusDto {
        name: st.manifest.name.clone(),
        dir: st
            .project_dir
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
        dirty: st.dirty,
        class_count: st.classes.len(),
        attached: st.target.as_ref().map(|t| AttachedDto::of(t)),
    }
}

fn remember_recent(st: &mut AppState, dir: &Path) {
    let dir = dir.to_string_lossy().into_owned();
    st.config.recent_projects.retain(|p| p != &dir);
    st.config.recent_projects.insert(0, dir);
    st.config.recent_projects.truncate(12);
}

/// Creates and opens a new empty project at `dir`.
#[tauri::command]
pub fn project_new(
    state: State<'_, Mutex<AppState>>,
    dir: String,
    name: String,
) -> Result<ProjectStatusDto, String> {
    let path = PathBuf::from(&dir);
    let loaded = project::create_dir(&path, &name).map_err(|e| e.to_string())?;
    let mut st = state.lock();
    st.manifest = loaded.manifest;
    st.classes = loaded.classes.classes;
    st.project_dir = Some(loaded.dir.clone());
    st.dirty = false;
    remember_recent(&mut st, &loaded.dir);
    Ok(status(&st))
}

/// Opens an existing project folder.
#[tauri::command]
pub fn project_open(
    state: State<'_, Mutex<AppState>>,
    dir: String,
) -> Result<ProjectStatusDto, String> {
    let path = PathBuf::from(&dir);
    let loaded = project::load_dir(&path).map_err(|e| e.to_string())?;
    let mut st = state.lock();
    st.manifest = loaded.manifest;
    st.classes = loaded.classes.classes;
    st.project_dir = Some(loaded.dir.clone());
    st.dirty = false;
    remember_recent(&mut st, &loaded.dir);
    Ok(status(&st))
}

/// Saves the current project to its directory.
#[tauri::command]
pub fn project_save(state: State<'_, Mutex<AppState>>) -> Result<ProjectStatusDto, String> {
    let mut st = state.lock();
    let dir = st
        .project_dir
        .clone()
        .ok_or("no project directory — use Save As")?;
    let classes = Project::from_types(st.classes.clone());
    project::save_dir(&dir, &st.manifest, &classes).map_err(|e| e.to_string())?;
    st.dirty = false;
    Ok(status(&st))
}

/// Saves the current project to a new directory and adopts it.
#[tauri::command]
pub fn project_save_as(
    state: State<'_, Mutex<AppState>>,
    dir: String,
) -> Result<ProjectStatusDto, String> {
    let path = PathBuf::from(&dir);
    let mut st = state.lock();
    let classes = Project::from_types(st.classes.clone());
    project::save_dir(&path, &st.manifest, &classes).map_err(|e| e.to_string())?;
    st.project_dir = Some(path.clone());
    st.dirty = false;
    remember_recent(&mut st, &path);
    Ok(status(&st))
}

/// Whether `dir` looks like a project folder.
#[tauri::command]
pub fn is_project_dir(dir: String) -> Result<bool, String> {
    Ok(project::is_project_dir(&PathBuf::from(dir)))
}

/// The current manifest (name + auto-attach).
#[tauri::command]
pub fn get_manifest(state: State<'_, Mutex<AppState>>) -> Result<Manifest, String> {
    Ok(state.lock().manifest.clone())
}

/// Replaces the manifest and marks the project dirty.
#[tauri::command]
pub fn set_manifest(
    state: State<'_, Mutex<AppState>>,
    manifest: Manifest,
) -> Result<(), String> {
    let mut st = state.lock();
    st.manifest = manifest;
    st.dirty = true;
    Ok(())
}

/// The current project/attachment status, for the toolbar.
#[tauri::command]
pub fn project_status(state: State<'_, Mutex<AppState>>) -> Result<ProjectStatusDto, String> {
    Ok(status(&state.lock()))
}
