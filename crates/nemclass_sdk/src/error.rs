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

    /// Project (RON/TOML) serialization/deserialization failed.
    #[error("project (de)serialization failed: {0}")]
    Project(String),

    /// A filesystem operation on a project folder failed.
    #[error("io error: {0}")]
    Io(String),

    /// A debugger control operation failed (attach, breakpoint, wait, ...).
    #[error("debug error: {0}")]
    Debug(String),

    /// A raw `ptrace` request failed.
    #[error("ptrace {op} failed (errno {errno})")]
    Ptrace {
        /// The ptrace request that failed (e.g. `"SEIZE"`, `"GETREGS"`).
        op: &'static str,
        /// The OS error number.
        errno: i32,
    },

    /// A memory scan failed to complete.
    #[error("scan error: {0}")]
    Scan(String),

    /// A pluggable backend reported an internal error.
    #[error("{name} backend error: {reason}")]
    Backend {
        /// Backend name (e.g. `"frida"`, `"intel-pt"`, `"libiht"`).
        name: &'static str,
        /// Human-readable failure reason.
        reason: String,
    },

    /// The requested backend/feature is not compiled in, or is unavailable on
    /// this host at runtime (missing kernel module, no PT support, ...).
    #[error("unsupported: {0}")]
    Unsupported(&'static str),
}

impl From<std::io::Error> for SdkError {
    fn from(e: std::io::Error) -> Self {
        SdkError::Io(e.to_string())
    }
}
