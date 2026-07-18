//! SDK error type.

use nemclass_memory::MfError;

/// Result alias used throughout the SDK.
pub type Result<T> = core::result::Result<T, SdkError>;

/// Errors surfaced by the headless SDK.
#[derive(Debug, thiserror::Error)]
pub enum SdkError {
    /// A low-level memory operation failed.
    #[error("memory error: {0}")]
    Memory(#[from] MfError),

    /// A memory read/write against the target failed (address unreadable, process died, ...).
    #[error("memory access failed at {address:#x}")]
    Access {
        /// Target address the failed access was aimed at.
        address: usize,
    },

    /// A managed plugin library could not be loaded or was missing an export.
    #[error("plugin error: {0}")]
    Plugin(String),

    /// A signature/pattern string could not be parsed.
    #[error("invalid pattern: {0}")]
    Pattern(String),

    /// An address expression could not be parsed or evaluated.
    #[error("invalid address expression: {0}")]
    Expr(String),

    /// A referenced module was not found in the target.
    #[error("module not found: {0}")]
    ModuleNotFound(String),

    /// Project (RON) serialization/deserialization failed.
    #[error("project (de)serialization failed: {0}")]
    Project(String),
}
