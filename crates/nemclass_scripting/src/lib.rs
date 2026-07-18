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
