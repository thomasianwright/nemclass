//! Address parsing. Delegates to the SDK's address-expression evaluator so plain
//! hex and arithmetic (`0x10+0x20`) resolve without a target. Module/deref syntax
//! (`<mod>`, `[expr]`) needs a live target and is available through
//! `Target::eval` / the scripting API instead.

use nemclass_sdk::offset::{eval, AddrEnv};
use nemclass_sdk::{Result as SdkResult, SdkError};

/// An [`AddrEnv`] with no attached process: numbers and arithmetic resolve,
/// module lookups and dereferences fail.
struct NoTarget;

impl AddrEnv for NoTarget {
    fn module_base(&self, name: &str) -> SdkResult<usize> {
        Err(SdkError::ModuleNotFound(name.into()))
    }

    fn deref(&self, _address: usize) -> Option<usize> {
        None
    }
}

/// Parses an address expression. All bare numbers are hexadecimal (`0x` optional).
pub fn parse_address(addr: &str) -> Option<usize> {
    eval(&NoTarget, addr).ok()
}
