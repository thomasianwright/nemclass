//! Signature patterns for memory scanning — a thin, fallible wrapper over
//! [`nemclass_memory::DynPattern`] so patterns can be built from runtime strings.

use crate::error::{Result, SdkError};
use nemclass_memory::{DynPattern, Matcher};

/// A parsed memory signature. Build one with [`Pattern::ida`], [`Pattern::peid`]
/// or [`Pattern::code`], then scan with [`crate::Target::scan_range`] /
/// [`crate::Target::scan_module`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern(DynPattern);

impl Pattern {
    /// Parses an IDA-style signature, e.g. `"48 8B ?? 33"`.
    pub fn ida(sig: &str) -> Result<Self> {
        DynPattern::from_ida_style(sig)
            .map(Self)
            .map_err(|e| SdkError::Pattern(e.to_string()))
    }

    /// Parses a PEID-style signature, e.g. `"48 8B ?? 33"` (`??` wildcards).
    pub fn peid(sig: &str) -> Result<Self> {
        DynPattern::from_peid_style(sig)
            .map(Self)
            .map_err(|e| SdkError::Pattern(e.to_string()))
    }

    /// Builds a pattern from a raw byte template and an `x`/`?` mask.
    pub fn code(bytes: &[u8], mask: &str) -> Result<Self> {
        DynPattern::from_code_style(bytes, mask)
            .map(Self)
            .map_err(|e| SdkError::Pattern(e.to_string()))
    }

    /// Number of bytes the pattern matches.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the pattern is empty (always `false` for a parsed pattern).
    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }
}

impl Matcher for Pattern {
    fn matches(&self, seq: &[u8]) -> bool {
        self.0.matches(seq)
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

impl Matcher for &Pattern {
    fn matches(&self, seq: &[u8]) -> bool {
        self.0.matches(seq)
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_match() {
        let p = Pattern::ida("11 ? 33").unwrap();
        assert_eq!(p.len(), 3);
        assert!(p.matches(b"\x11\x22\x33"));
        assert!(Pattern::ida("zz").is_err());
    }
}
