//! `nemclass-cli` — run nemclass Lua scripts headlessly (no GUI).
//!
//! ```text
//! nemclass-cli run <script.lua> [--pid N | --name PROC] [--project DIR]
//! ```
//!
//! With `--project DIR`:
//! - a bare `<script.lua>` is resolved under `DIR/scripts/` if not a direct path,
//! - `PROJECT` is exposed to the script as the project directory, and
//! - if neither `--pid` nor `--name` is given and the manifest has an
//!   `[auto_attach]`, its resolved process id is exposed as `PID`.
//!
//! The target selector reaches the script as the globals `PID` / `PNAME`:
//!
//! ```lua
//! local proc = PID and nem.open(PID) or nem.attach{ name = PNAME }
//! ```

use nemclass_scripting::ScriptEngine;
use nemclass_sdk::{project, Target};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str =
    "usage: nemclass-cli run <script.lua> [--pid N | --name PROC] [--project DIR]";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);

    match args.next().as_deref() {
        Some("run") => {}
        Some("-h") | Some("--help") | None => {
            println!("{USAGE}");
            return Ok(());
        }
        Some(other) => return Err(format!("unknown command `{other}`\n{USAGE}").into()),
    }

    let script_arg = args.next().ok_or(format!("missing <script.lua>\n{USAGE}"))?;

    let mut pid: Option<u32> = None;
    let mut name: Option<String> = None;
    let mut project_dir: Option<PathBuf> = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--pid" => {
                pid = Some(
                    args.next()
                        .ok_or("--pid needs a value")?
                        .parse()
                        .map_err(|_| "--pid must be a number")?,
                )
            }
            "--name" => name = Some(args.next().ok_or("--name needs a value")?),
            "--project" => project_dir = Some(PathBuf::from(args.next().ok_or("--project needs a value")?)),
            other => return Err(format!("unknown flag `{other}`\n{USAGE}").into()),
        }
    }

    // Resolve the script: a direct path, else `<project>/scripts/<name>`.
    let script = resolve_script(&script_arg, project_dir.as_deref())?;

    // If no explicit selector and the project auto-attaches, resolve its pid.
    if pid.is_none() && name.is_none() {
        if let Some(dir) = &project_dir {
            if let Some(spec) = project::read_manifest(dir).ok().and_then(|m| m.auto_attach) {
                match Target::from_auto_attach(&spec) {
                    Ok(target) => pid = Some(target.id()),
                    Err(e) => eprintln!("warning: auto-attach failed: {e}"),
                }
            }
        }
    }

    let engine = ScriptEngine::new()?;
    if let Some(dir) = &project_dir {
        engine
            .lua()
            .globals()
            .set("PROJECT", dir.to_string_lossy().to_string())?;
    }
    if let Some(pid) = pid {
        engine.lua().globals().set("PID", pid)?;
    }
    if let Some(name) = name {
        engine.lua().globals().set("PNAME", name)?;
    }

    engine.run_file(&script)?;
    Ok(())
}

/// Returns `script_arg` if it is an existing file; otherwise, when a project is
/// given, tries `<project>/scripts/<script_arg>`.
fn resolve_script(script_arg: &str, project_dir: Option<&Path>) -> Result<PathBuf, String> {
    let direct = PathBuf::from(script_arg);
    if direct.is_file() {
        return Ok(direct);
    }
    if let Some(dir) = project_dir {
        let in_scripts = dir.join(project::SCRIPTS_DIR).join(script_arg);
        if in_scripts.is_file() {
            return Ok(in_scripts);
        }
    }
    Err(format!("script not found: {script_arg}"))
}
