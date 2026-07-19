//! Tauri command modules, grouped by UI surface. Each command is thin glue over
//! `nemclass_sdk` and returns `Result<T, String>` (the error is shown as a toast).

pub mod classes;
pub mod inspect;
pub mod process;
pub mod project;
