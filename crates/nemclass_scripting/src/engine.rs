//! The [`ScriptEngine`]: owns a Lua state with the `nem` API registered.

use mlua::Lua;
use std::path::Path;

/// Errors from loading or running a script.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    /// A Lua runtime/compile error, or an SDK error surfaced through Lua.
    #[error(transparent)]
    Lua(#[from] mlua::Error),
    /// Reading a script file failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Convenience result alias.
pub type Result<T> = std::result::Result<T, ScriptError>;

/// A Lua interpreter with the `nem` scripting API installed.
pub struct ScriptEngine {
    lua: Lua,
}

impl ScriptEngine {
    /// Creates a fresh engine with the `nem` global registered.
    pub fn new() -> Result<Self> {
        let lua = Lua::new();
        crate::bindings::register(&lua)?;
        Ok(Self { lua })
    }

    /// Access the underlying Lua state (e.g. to install extra globals from the host).
    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    /// Runs a chunk of Lua source.
    pub fn run_str(&self, code: &str) -> Result<()> {
        self.lua.load(code).exec()?;
        Ok(())
    }

    /// Runs a chunk and returns its value converted to `T`.
    pub fn eval_str<T: mlua::FromLua>(&self, code: &str) -> Result<T> {
        Ok(self.lua.load(code).eval()?)
    }

    /// Runs a Lua script file.
    pub fn run_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let code = std::fs::read_to_string(path)?;
        self.run_str(&code)
    }
}
