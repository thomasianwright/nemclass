//! The canonical backend state — the Tauri equivalent of the egui GUI's
//! `GlobalState`, held behind a `Mutex` via Tauri's managed state.

use crate::debugger::{AccessHandle, DebuggerHandle};
use crate::spider::SpiderHandle;
use nemclass_sdk::project::Manifest;
use nemclass_sdk::scan::ScanResults;
use nemclass_sdk::schema::TypeDef;
use nemclass_sdk::table::{CheatTable, Freezer};
use nemclass_sdk::Target;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
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

impl Config {
    /// Loads `config.json` from `dir`, falling back to defaults.
    pub fn load(dir: &Path) -> Self {
        std::fs::read_to_string(dir.join("config.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Writes `config.json` into `dir` (creating it if needed).
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let text = serde_json::to_string_pretty(self).unwrap_or_default();
        std::fs::write(dir.join("config.json"), text)
    }
}

/// The whole backend state.
pub struct AppState {
    /// Project manifest (name + auto-attach).
    pub manifest: Manifest,
    /// The canonical class layouts — mutated directly for all CRUD.
    pub classes: Vec<TypeDef>,
    /// Per-class base addresses set/read by Lua scripts (nem.class_address).
    pub class_addresses: HashMap<String, usize>,
    /// The open project directory, if any.
    pub project_dir: Option<PathBuf>,
    /// Whether there are unsaved changes.
    pub dirty: bool,
    /// The attached target, shared with background job threads.
    pub target: Option<Arc<Target>>,
    /// The cheat table (freeze/watch entries).
    pub cheat_table: CheatTable,
    /// Background value freezer, live while attached.
    pub freezer: Option<Freezer>,
    /// The latest value-scan result set (for `next_scan`).
    pub scan_results: Option<ScanResults>,
    /// The current structure-spider search, if any.
    pub spider: Option<SpiderHandle>,
    /// The attached debugger (runs on its own thread), if any.
    pub debugger: Option<DebuggerHandle>,
    /// The running access tracer, if any.
    pub access: Option<AccessHandle>,
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
            class_addresses: HashMap::new(),
            project_dir: None,
            dirty: false,
            target: None,
            cheat_table: CheatTable::new(),
            freezer: None,
            scan_results: None,
            spider: None,
            debugger: None,
            access: None,
            config: Config::default(),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Attaches `target`: shares it via `Arc`, spins up a running [`Freezer`], and
    /// re-pins any already-frozen cheat-table entries.
    pub fn set_target(&mut self, target: Target) {
        let arc = Arc::new(target);
        let mut freezer = Freezer::new(arc.clone());
        freezer.start();
        self.target = Some(arc);
        self.freezer = Some(freezer);
        self.resync_freezer();
    }

    /// Detaches: drops the freezer (its `Drop` stops the thread) and the target,
    /// and discards now-stale scan results.
    pub fn clear_target(&mut self) {
        self.access = None;
        self.debugger = None;
        self.freezer = None;
        self.target = None;
        self.scan_results = None;
        self.spider = None;
    }

    /// Rebuilds the freezer's pinned set from the cheat table's frozen entries.
    pub fn resync_freezer(&mut self) {
        if let Some(fz) = &self.freezer {
            fz.clear();
            for e in self.cheat_table.all_entries() {
                if let Some(fv) = &e.freeze {
                    fz.freeze(e.address.clone(), fv.bytes.clone());
                }
            }
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
