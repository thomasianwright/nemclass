//! `nemclass-cli` — run nemclass Lua scripts headlessly (no GUI).
//!
//! ```text
//! nemclass-cli run <script.lua> [--pid N | --name PROC]
//! ```
//!
//! When `--pid`/`--name` is given, the target selector is exposed to the script
//! as the globals `PID` (integer) and/or `PNAME` (string), so a script can do:
//!
//! ```lua
//! local proc = PID and nem.open(PID) or nem.attach{ name = PNAME }
//! ```

use nemclass_scripting::ScriptEngine;
use std::process::ExitCode;

const USAGE: &str = "usage: nemclass-cli run <script.lua> [--pid N | --name PROC]";

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

    let script = args.next().ok_or(format!("missing <script.lua>\n{USAGE}"))?;

    let mut pid: Option<u32> = None;
    let mut name: Option<String> = None;
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
            other => return Err(format!("unknown flag `{other}`\n{USAGE}").into()),
        }
    }

    let engine = ScriptEngine::new()?;
    if let Some(pid) = pid {
        engine.lua().globals().set("PID", pid)?;
    }
    if let Some(name) = name {
        engine.lua().globals().set("PNAME", name)?;
    }

    engine.run_file(&script)?;
    Ok(())
}
