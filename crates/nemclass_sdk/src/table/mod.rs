//! Cheat tables — persisted address + type + freeze entries, and a background
//! [`Freezer`] that keeps frozen values written.
//!
//! Entries address memory through [`offset::eval`](crate::offset::eval)
//! expressions (module-relative, pointer-chained: `"[<game.exe>+0x1A2B]+0x10"`)
//! and type them with [`FieldKind`], so tables interoperate with the class schema.
//! Tables persist as RON under a project's `tables/` folder (see
//! [`project::TABLES_DIR`](crate::project::TABLES_DIR)).

use crate::error::{Result, SdkError};
use crate::offset;
use crate::target::Target;
use crate::types::FieldKind;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// Serializes [`FieldKind`] as its compact string form (`"I32"`, `"Vec3f"`, …),
/// matching how the class schema persists kinds.
mod kind_serde {
    use super::FieldKind;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(kind: &FieldKind, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&kind.to_kind_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<FieldKind, D::Error> {
        let s = String::deserialize(d)?;
        FieldKind::from_kind_string(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown field kind `{s}`")))
    }
}

/// A value pinned by the [`Freezer`]: the exact bytes to keep writing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FreezeValue {
    /// Native-endian bytes written back on every freeze tick.
    pub bytes: Vec<u8>,
}

/// What a hotkey does when pressed (interpreted by the host/GUI, stored here).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HotkeyAction {
    /// Toggle this entry's freeze on/off.
    ToggleFreeze,
    /// Set the entry to a literal value (parsed per the entry's [`FieldKind`]).
    SetValue(String),
}

/// A key binding attached to an entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hotkey {
    /// Human-readable key combo (e.g. `"Ctrl+F1"`); interpreted by the host.
    pub combo: String,
    /// The action to perform.
    pub action: HotkeyAction,
}

/// One cheat-table row: a described, typed address with optional freeze, hotkey,
/// group label, and nested children (CE-style folders).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheatEntry {
    /// Human-readable description.
    pub description: String,
    /// Address expression, resolved via [`offset::eval`].
    pub address: String,
    /// Value type at the address.
    #[serde(with = "kind_serde")]
    pub kind: FieldKind,
    /// When set, the value is held to these bytes by the [`Freezer`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freeze: Option<FreezeValue>,
    /// Optional key binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<Hotkey>,
    /// Optional group/folder label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Nested child entries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<CheatEntry>,
}

impl CheatEntry {
    /// A minimal entry: description, address expression, and type.
    pub fn new(description: impl Into<String>, address: impl Into<String>, kind: FieldKind) -> Self {
        Self {
            description: description.into(),
            address: address.into(),
            kind,
            freeze: None,
            hotkey: None,
            group: None,
            children: Vec::new(),
        }
    }

    /// Resolves this entry's address in `target` (following its pointer chain).
    pub fn resolve(&self, target: &Target) -> Result<usize> {
        offset::eval(target, &self.address)
    }

    /// Reads the entry's current raw value bytes from `target`.
    pub fn read_bytes(&self, target: &Target) -> Option<Vec<u8>> {
        let addr = self.resolve(target).ok()?;
        target.read_bytes(addr, self.kind.size_with_ptr(target.pointer_size()))
    }

    /// Yields this entry and all descendants, depth-first.
    pub fn flatten(&self) -> Vec<&CheatEntry> {
        let mut out = vec![self];
        for c in &self.children {
            out.extend(c.flatten());
        }
        out
    }
}

/// A cheat table: a versioned list of [`CheatEntry`] trees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheatTable {
    /// On-disk schema version.
    pub version: u32,
    /// Top-level entries.
    pub entries: Vec<CheatEntry>,
}

impl Default for CheatTable {
    fn default() -> Self {
        Self::new()
    }
}

impl CheatTable {
    /// The current on-disk schema version.
    pub const CURRENT_VERSION: u32 = 1;

    /// An empty table at the current version.
    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            entries: Vec::new(),
        }
    }

    /// Every entry, flattened depth-first (parents before children).
    pub fn all_entries(&self) -> Vec<&CheatEntry> {
        self.entries.iter().flat_map(|e| e.flatten()).collect()
    }

    /// Parses a table from RON text.
    pub fn from_ron(text: &str) -> Result<Self> {
        ron::from_str(text).map_err(|e| SdkError::Project(e.to_string()))
    }

    /// Serializes the table to pretty RON (human-editable).
    pub fn to_ron(&self) -> Result<String> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|e| SdkError::Project(e.to_string()))
    }

    /// Loads a table from a `.ron` file.
    pub fn load(path: &Path) -> Result<Self> {
        Self::from_ron(&std::fs::read_to_string(path)?)
    }

    /// Writes the table to a `.ron` file.
    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, self.to_ron()?)?;
        Ok(())
    }
}

/// A pinned value tracked by a running [`Freezer`].
struct FrozenItem {
    id: u64,
    expr: String,
    bytes: Vec<u8>,
}

/// Background writer that holds frozen entries at their pinned values by
/// re-resolving each address and writing its bytes on a fixed interval.
///
/// Shares the [`Target`] via `Arc` (targets are `Send + Sync`), so freezing runs
/// concurrently with scanning and inspection.
pub struct Freezer {
    target: Arc<Target>,
    frozen: Arc<Mutex<Vec<FrozenItem>>>,
    stop: Arc<AtomicBool>,
    next_id: AtomicU64,
    interval: Duration,
    handle: Option<JoinHandle<()>>,
}

impl Freezer {
    /// A freezer over `target` with the default 50 ms tick (not yet running).
    pub fn new(target: Arc<Target>) -> Self {
        Self::with_interval(target, Duration::from_millis(50))
    }

    /// A freezer with a custom tick interval.
    pub fn with_interval(target: Arc<Target>, interval: Duration) -> Self {
        Self {
            target,
            frozen: Arc::new(Mutex::new(Vec::new())),
            stop: Arc::new(AtomicBool::new(false)),
            next_id: AtomicU64::new(1),
            interval,
            handle: None,
        }
    }

    /// Pins `bytes` at address expression `expr`; returns a handle for [`Self::unfreeze`].
    pub fn freeze(&self, expr: impl Into<String>, bytes: Vec<u8>) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.frozen.lock().unwrap().push(FrozenItem { id, expr: expr.into(), bytes });
        id
    }

    /// Convenience: freeze an entry at its current [`FreezeValue`] bytes.
    pub fn freeze_entry(&self, entry: &CheatEntry) -> Option<u64> {
        let bytes = entry.freeze.as_ref()?.bytes.clone();
        Some(self.freeze(entry.address.clone(), bytes))
    }

    /// Removes a frozen value by its handle.
    pub fn unfreeze(&self, id: u64) {
        self.frozen.lock().unwrap().retain(|it| it.id != id);
    }

    /// Removes all frozen values.
    pub fn clear(&self) {
        self.frozen.lock().unwrap().clear();
    }

    /// The number of currently frozen values.
    pub fn len(&self) -> usize {
        self.frozen.lock().unwrap().len()
    }

    /// Whether nothing is currently frozen.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Starts the background writer (no-op if already running).
    pub fn start(&mut self) {
        if self.handle.is_some() {
            return;
        }
        self.stop.store(false, Ordering::SeqCst);
        let target = self.target.clone();
        let frozen = self.frozen.clone();
        let stop = self.stop.clone();
        let interval = self.interval;
        self.handle = Some(std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                {
                    let items = frozen.lock().unwrap();
                    for it in items.iter() {
                        if let Ok(addr) = offset::eval(&*target, &it.expr) {
                            let _ = target.write(addr, &it.bytes);
                        }
                    }
                }
                std::thread::sleep(interval);
            }
        }));
    }

    /// Stops the background writer and joins its thread.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Freezer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_table() -> CheatTable {
        let mut t = CheatTable::new();
        let mut health = CheatEntry::new("Health", "[<game.exe>+0x1A2B]+0x10", FieldKind::I32);
        health.freeze = Some(FreezeValue { bytes: 100i32.to_ne_bytes().to_vec() });
        health.hotkey = Some(Hotkey {
            combo: "Ctrl+F1".into(),
            action: HotkeyAction::ToggleFreeze,
        });
        health.group = Some("Player".into());
        health.children.push(CheatEntry::new(
            "Max Health",
            "[<game.exe>+0x1A2B]+0x14",
            FieldKind::I32,
        ));
        t.entries.push(health);
        t.entries
            .push(CheatEntry::new("Position", "[<game.exe>+0x2000]", FieldKind::Vector {
                components: 3,
                width: crate::types::FloatWidth::F32,
            }));
        t
    }

    #[test]
    fn ron_roundtrip() {
        let table = sample_table();
        let text = table.to_ron().unwrap();
        assert!(text.contains("kind: \"I32\""), "{text}");
        assert!(text.contains("kind: \"Vec3f\""), "{text}");
        let back = CheatTable::from_ron(&text).unwrap();
        assert_eq!(back, table);
    }

    #[test]
    fn flatten_counts_children() {
        let table = sample_table();
        // Health + Max Health + Position == 3.
        assert_eq!(table.all_entries().len(), 3);
    }
}
