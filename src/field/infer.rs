//! Heuristic type inference now lives in the headless SDK. Re-exported so
//! existing `crate::field::infer_*` call sites keep working; the SDK functions
//! take a `&Target`, and a `&Process` derefs to it at the call site.

pub use nemclass_sdk::infer::{infer_float_hint, infer_kind, read_window};
