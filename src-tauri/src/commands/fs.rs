//! Filesystem browsing for the in-app directory picker (replaces the GTK native
//! folder dialog, which crashes on systems with a broken SVG pixbuf loader).

use nemclass_sdk::project;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntryDto {
    pub name: String,
    pub path: String,
    pub is_project: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirListingDto {
    pub path: String,
    pub parent: Option<String>,
    pub dirs: Vec<DirEntryDto>,
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Lists sub-directories of `path` (default: home), marking project folders.
#[tauri::command]
pub fn list_dir(path: Option<String>) -> Result<DirListingDto, String> {
    let dir = path.map(PathBuf::from).filter(|p| p.is_dir()).unwrap_or_else(home);

    let mut dirs = Vec::new();
    let rd = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue; // skip hidden
        }
        dirs.push(DirEntryDto {
            is_project: project::is_project_dir(&p),
            path: p.to_string_lossy().into_owned(),
            name,
        });
    }
    dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(DirListingDto {
        parent: dir.parent().map(|p| p.to_string_lossy().into_owned()),
        path: dir.to_string_lossy().into_owned(),
        dirs,
    })
}

/// The user's home directory.
#[tauri::command]
pub fn home_dir() -> Result<String, String> {
    Ok(home().to_string_lossy().into_owned())
}
