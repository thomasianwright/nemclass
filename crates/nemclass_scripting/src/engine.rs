//! The [`ScriptEngine`]: owns a Lua state with the `nem` API registered.

use crate::ClassHost;
use mlua::Lua;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

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

    /// Runs `code` for an interactive console: `print` is captured (rather than
    /// going to stdout), an attached process `pid` is exposed as the `PID`
    /// global, and after the run the `EXPORT` global (if a string) is returned so
    /// the host can import a project.
    ///
    /// Returns the captured output alongside the run result, so callers can show
    /// partial output even when the script errors.
    pub fn run_console(&self, code: &str, pid: Option<u32>) -> (String, Result<Option<String>>) {
        let out = Rc::new(RefCell::new(String::new()));
        let sink = out.clone();

        let install = (|| -> mlua::Result<()> {
            let print = self.lua.create_function(move |lua, args: mlua::MultiValue| {
                let tostring: mlua::Function = lua.globals().get("tostring")?;
                let mut line = String::new();
                for (i, v) in args.into_iter().enumerate() {
                    if i > 0 {
                        line.push('\t');
                    }
                    let s: String = tostring.call(v)?;
                    line.push_str(&s);
                }
                line.push('\n');
                sink.borrow_mut().push_str(&line);
                Ok(())
            })?;
            self.lua.globals().set("print", print)?;
            if let Some(pid) = pid {
                self.lua.globals().set("PID", pid)?;
            }
            Ok(())
        })();

        if let Err(e) = install {
            let printed = out.borrow().clone();
            return (printed, Err(e.into()));
        }

        let result = self
            .lua
            .load(code)
            .exec()
            .and_then(|()| self.lua.globals().get::<Option<String>>("EXPORT"))
            .map_err(ScriptError::from);

        let printed = out.borrow().clone();
        (printed, result)
    }

    /// Like [`run_console`](Self::run_console), but also binds `nem.classes`,
    /// `nem.set_class_address` and `nem.class_address` to `host` so the script
    /// can read and drive the embedding app's classes.
    pub fn run_console_with_host<H: ClassHost + Clone + 'static>(
        &self,
        code: &str,
        pid: Option<u32>,
        host: H,
    ) -> (String, Result<Option<String>>) {
        if let Err(e) = self.register_class_host(host) {
            return (String::new(), Err(e.into()));
        }
        self.run_console(code, pid)
    }

    /// Installs the `nem.classes` / `nem.set_class_address` / `nem.class_address`
    /// bindings backed by `host`, replacing the CLI stubs.
    fn register_class_host<H: ClassHost + Clone + 'static>(&self, host: H) -> mlua::Result<()> {
        let nem: mlua::Table = self.lua.globals().get("nem")?;

        let h = host.clone();
        nem.set(
            "classes",
            self.lua.create_function(move |_, ()| Ok(h.class_names()))?,
        )?;

        let h = host.clone();
        nem.set(
            "class_address",
            self.lua
                .create_function(move |_, name: String| Ok(h.class_address(&name).map(|a| a as i64)))?,
        )?;

        let h = host;
        nem.set(
            "set_class_address",
            self.lua
                .create_function(move |_, (name, addr): (String, i64)| {
                    if h.set_class_address(&name, addr as usize) {
                        Ok(())
                    } else {
                        Err(mlua::Error::RuntimeError(format!("no class named `{name}`")))
                    }
                })?,
        )?;

        Ok(())
    }
}
