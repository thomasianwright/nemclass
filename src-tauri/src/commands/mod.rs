//! Tauri command modules, grouped by UI surface. Each command is thin glue over
//! `nemclass_sdk` and returns `Result<T, String>` (the error is shown as a toast).

pub mod classes;
pub mod disasm;
pub mod generator;
pub mod inspect;
pub mod process;
pub mod project;
pub mod scan;
pub mod spider;
pub mod table;
