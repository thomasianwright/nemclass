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
