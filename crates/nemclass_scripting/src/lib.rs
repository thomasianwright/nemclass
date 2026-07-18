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
/// Language Server offers `nem.*` autocompletion: `scripts/nem.lua` (the
/// definitions) and a `.luarc.json` at the project root.
pub fn write_editor_support(project_dir: &Path) -> std::io::Result<()> {
    let scripts = project_dir.join(nemclass_sdk::project::SCRIPTS_DIR);
    std::fs::create_dir_all(&scripts)?;
    std::fs::write(scripts.join("nem.lua"), DEFINITIONS)?;
    std::fs::write(project_dir.join(".luarc.json"), LUARC)?;
    Ok(())
}
