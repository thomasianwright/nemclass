//! # nemclass_sdk
//!
//! Headless SDK for nemclass. Provides everything the GUI does *except*
//! rendering: attaching to a process ([`Target`]), IDA-style pattern scanning
//! ([`pattern`]), pointer-chain and address-expression resolution ([`offset`]),
//! the type-declaration schema ([`schema`]) with code generation
//! ([`generator`]), and heuristic type inference ([`infer`]).
//!
//! This crate never depends on egui/eframe, so it can be used from a CLI, a
//! script host, or tests.
#![warn(missing_docs)]

mod error;
pub use error::*;

pub mod types;
pub use types::{FieldKind, FloatWidth};

pub mod schema;
pub use schema::{FieldDef, Project, TypeBuilder, TypeDef};

pub mod project;
pub use project::{AutoAttach, LoadedProject, Manifest};

pub mod generator;
pub use generator::{generate, Generator, Lang};

pub mod pattern;
pub use pattern::Pattern;

pub mod offset;

pub mod target;
pub use target::Target;

pub mod infer;

pub mod debug;
pub use debug::{
    BackendKind, BpId, DebugEvent, Debugger, Registers, StopReason, ThreadId, WatchKind, WatchSize,
};

pub mod scan;
pub use scan::{ScanCompare, ScanConfig, ScanResults, ScanType, ScanValue, Scanner};

pub mod table;
pub use table::{CheatEntry, CheatTable, Freezer};

pub mod access;
pub use access::{find_what_accesses, AccessBackend, AccessRecord, AccessTracer};

#[cfg(feature = "disasm")]
pub mod disasm;
#[cfg(feature = "disasm")]
pub use disasm::{
    call_targets, disassemble, find_functions, find_strings, memory_map, FlowKind, Insn, MapRegion,
    RegionKind, StringHit,
};

/// Re-export of the low-level memory crate for advanced callers.
pub use nemclass_memory;
