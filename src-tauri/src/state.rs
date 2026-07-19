//! The canonical backend state — the Tauri equivalent of the egui GUI's
//! `GlobalState`, held behind a `Mutex` via Tauri's managed state.

use nemclass_sdk::project::Manifest;
use nemclass_sdk::schema::TypeDef;
use nemclass_sdk::Target;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

/// How deep the undo/redo history goes.
const UNDO_LIMIT: usize = 200;

/// Persisted user/session configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Recently opened project directories, most-recent first.
    pub recent_projects: Vec<String>,
    /// Inspector live-value polling frequency, in Hz.
    pub poll_hz: u32,
    /// Serialized dockview layout (opaque JSON), if the user has customised it.
    pub layout: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            recent_projects: Vec::new(),
            poll_hz: 30,
            layout: None,
        }
    }
}

/// The whole backend state.
pub struct AppState {
    /// Project manifest (name + auto-attach).
    pub manifest: Manifest,
    /// The canonical class layouts — mutated directly for all CRUD.
    pub classes: Vec<TypeDef>,
    /// The open project directory, if any.
    pub project_dir: Option<PathBuf>,
    /// Whether there are unsaved changes.
    pub dirty: bool,
    /// The attached target, shared with background job threads.
    pub target: Option<Arc<Target>>,
    /// Persisted configuration.
    pub config: Config,

    undo: Vec<Vec<TypeDef>>,
    redo: Vec<Vec<TypeDef>>,
}

impl AppState {
    /// A fresh, empty state.
    pub fn new() -> Self {
        Self {
            manifest: Manifest::new("Untitled"),
            classes: Vec::new(),
            project_dir: None,
            dirty: false,
            target: None,
            config: Config::default(),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// The target's pointer width, or the 64-bit default when not attached.
    pub fn ptr_size(&self) -> usize {
        self.target
            .as_ref()
            .map(|t| t.pointer_size())
            .unwrap_or(nemclass_sdk::types::DEFAULT_PTR_SIZE)
    }

    /// Records a snapshot of the current class layouts for undo, clears the redo
    /// stack, and marks the project dirty. Call *before* mutating `classes`.
    pub fn snapshot(&mut self) {
        self.undo.push(self.classes.clone());
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }

    /// Reverts to the previous snapshot; returns `false` if there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(prev) => {
                self.redo.push(std::mem::replace(&mut self.classes, prev));
                self.dirty = true;
                true
            }
            None => false,
        }
    }

    /// Re-applies the last undone change; returns `false` if there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(next) => {
                self.undo.push(std::mem::replace(&mut self.classes, next));
                self.dirty = true;
                true
            }
            None => false,
        }
    }

    /// Index of a class by name.
    pub fn class_index(&self, name: &str) -> Option<usize> {
        self.classes.iter().position(|c| c.name == name)
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
