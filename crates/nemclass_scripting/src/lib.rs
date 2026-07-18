//! # nemclass_scripting
//!
//! A Lua (5.4) scripting host over [`nemclass_sdk`]. Exposes a `nem` global with
//! process enumeration/attachment, memory reads/writes, IDA pattern scanning,
//! pointer/offset resolution (Wine-aware), and a type-declaration + codegen API.
//!
//! ```no_run
//! # fn main() -> Result<(), nemclass_scripting::ScriptError> {
//! let engine = nemclass_scripting::ScriptEngine::new()?;
//! engine.run_str(r#"
//!     local proc = nem.attach{ name = "subject" }
//!     local addr = proc:eval("<subject>+0x1000")
//!     print(("value = %d"):format(proc:read_i32(addr)))
//! "#)?;
//! # Ok(()) }
//! ```

mod bindings;
mod engine;

pub use engine::{Result, ScriptEngine, ScriptError};

use std::path::Path;

/// Host hook that lets scripts read and drive the embedding app's classes — e.g.
/// the GUI's live class list. Registered by
/// [`ScriptEngine::run_console_with_host`], which backs `nem.classes`,
/// `nem.set_class_address` and `nem.class_address` with it.
pub trait ClassHost {
    /// Names of the host's classes, in list order.
    fn class_names(&self) -> Vec<String>;

    /// Sets the base address of the class named `name`. Returns `false` if there
    /// is no such class.
    fn set_class_address(&self, name: &str, address: usize) -> bool;

    /// The current base address of the class named `name`, if it exists.
    fn class_address(&self, name: &str) -> Option<usize>;
}

#[cfg(test)]
mod host_tests {
    use super::{ClassHost, ScriptEngine};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    #[derive(Clone)]
    struct MockHost(Rc<RefCell<HashMap<String, usize>>>);

    impl ClassHost for MockHost {
        fn class_names(&self) -> Vec<String> {
            let mut v: Vec<_> = self.0.borrow().keys().cloned().collect();
            v.sort();
            v
        }
        fn set_class_address(&self, name: &str, address: usize) -> bool {
            match self.0.borrow_mut().get_mut(name) {
                Some(slot) => {
                    *slot = address;
                    true
                }
                None => false,
            }
        }
        fn class_address(&self, name: &str) -> Option<usize> {
            self.0.borrow().get(name).copied()
        }
    }

    #[test]
    fn host_bindings_read_and_write_classes() {
        let host = MockHost(Rc::new(RefCell::new(HashMap::from([("Player".to_string(), 0usize)]))));
        let engine = ScriptEngine::new().unwrap();
        let (_out, res) = engine.run_console_with_host(
            r#"
                assert(#nem.classes() == 1, "expected one class")
                nem.set_class_address("Player", 0xDEAD0000)
                EXPORT = tostring(nem.class_address("Player"))
            "#,
            None,
            host.clone(),
        );
        let export = res.unwrap();
        assert_eq!(host.0.borrow()["Player"], 0xDEAD0000);
        assert_eq!(export.as_deref(), Some(format!("{}", 0xDEAD0000u64).as_str()));
    }

    #[test]
    fn set_missing_class_errors() {
        let host = MockHost(Rc::new(RefCell::new(HashMap::new())));
        let engine = ScriptEngine::new().unwrap();
        let (_out, res) =
            engine.run_console_with_host(r#"nem.set_class_address("Nope", 1)"#, None, host);
        assert!(res.is_err());
    }

    #[test]
    fn gui_bindings_error_without_host() {
        let engine = ScriptEngine::new().unwrap();
        assert!(engine.run_str(r#"nem.classes()"#).is_err());
    }
}

#[cfg(test)]
mod editor_support_tests {
    use super::{write_editor_support, DEFINITIONS};

    #[test]
    fn refreshes_definitions_but_preserves_existing_luarc() {
        let dir = std::env::temp_dir().join("nemclass_editor_support_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // A pre-existing, customized .luarc.json and a stale nem.lua.
        let custom = "{ \"custom\": true }";
        std::fs::write(dir.join(".luarc.json"), custom).unwrap();
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("scripts/nem.lua"), "-- stale").unwrap();

        write_editor_support(&dir).unwrap();

        // nem.lua is refreshed to the bundled definitions...
        let defs = std::fs::read_to_string(dir.join("scripts/nem.lua")).unwrap();
        assert_eq!(defs, DEFINITIONS);
        // ...but the existing .luarc.json is left untouched.
        assert_eq!(std::fs::read_to_string(dir.join(".luarc.json")).unwrap(), custom);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_luarc_when_absent() {
        let dir = std::env::temp_dir().join("nemclass_editor_support_test2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        write_editor_support(&dir).unwrap();
        assert!(dir.join(".luarc.json").is_file());
        assert!(dir.join("scripts/nem.lua").is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// LuaCATS type definitions for the `nem` API. Point lua-language-server at this
/// (see [`write_editor_support`]) to get autocompletion and hover docs.
pub const DEFINITIONS: &str = include_str!("../nem.lua");

/// `.luarc.json` that loads [`DEFINITIONS`] from `scripts/` and declares the
/// host-provided globals so they are not flagged as undefined.
const LUARC: &str = r#"{
  "runtime.version": "Lua 5.4",
  "workspace.library": ["scripts"],
  "diagnostics.globals": ["nem", "PID", "PNAME", "PROJECT", "EXPORT"]
}
"#;

/// Writes editor-support files into `project_dir` so an editor with the Lua
/// Language Server offers `nem.*` autocompletion.
///
/// `scripts/nem.lua` (the generated definitions) is always refreshed. The
/// `.luarc.json` at the project root may be customized by the user, so it is
/// only written when it does not already exist — re-running this never clobbers
/// an existing config.
pub fn write_editor_support(project_dir: &Path) -> std::io::Result<()> {
    let scripts = project_dir.join(nemclass_sdk::project::SCRIPTS_DIR);
    std::fs::create_dir_all(&scripts)?;

    // Generated — always refresh so definitions track the binary.
    std::fs::write(scripts.join("nem.lua"), DEFINITIONS)?;

    // User-editable — write only when absent so customizations survive.
    let luarc = project_dir.join(".luarc.json");
    if !luarc.exists() {
        std::fs::write(luarc, LUARC)?;
    }

    Ok(())
}
