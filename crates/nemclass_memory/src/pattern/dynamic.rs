//! Runtime (heap-allocated) memory patterns.
//!
//! Unlike [`Pattern<N>`](crate::Pattern), whose length is a compile-time const,
//! [`DynPattern`] is built at runtime from strings whose length is not known
//! until then — e.g. an IDA signature typed into a script or a config file.

extern crate alloc;
use alloc::{string::String, vec::Vec};

use super::ByteMatch;
use crate::Matcher;

/// A runtime-built sequence of [`ByteMatch`]es.
///
/// ```
/// # use nemclass_memory::{DynPattern, Matcher};
/// let pat = DynPattern::from_ida_style("11 ? 33").unwrap();
/// assert!(pat.matches(b"\x11\x22\x33"));
/// assert!(!pat.matches(b"\x11\x22\x44"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DynPattern(Vec<ByteMatch>);

/// Error returned when a pattern string cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternParseError(String);

impl core::fmt::Display for PatternParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "invalid pattern: {}", self.0)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for PatternParseError {}

fn hex_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(10 + c - b'a'),
        b'A'..=b'F' => Some(10 + c - b'A'),
        _ => None,
    }
}

impl DynPattern {
    /// Builds a pattern from a list of [`ByteMatch`]es.
    pub fn new(bytes: Vec<ByteMatch>) -> Self {
        Self(bytes)
    }

    /// Underlying matches.
    pub fn as_slice(&self) -> &[ByteMatch] {
        &self.0
    }

    /// Parses an IDA-style signature, e.g. `"48 8B ?? 33"` or `"48 8B ? 33"`.
    ///
    /// Tokens are whitespace-separated. A token beginning with `?` is a
    /// wildcard (both single `?` and double `??` are accepted); every other
    /// token must be exactly two hex digits.
    pub fn from_ida_style(pat: &str) -> Result<Self, PatternParseError> {
        Self::parse_sig(pat)
    }

    /// Parses a PEID-style signature. Accepts the same tolerant grammar as
    /// [`Self::from_ida_style`] (`??` wildcards); provided for API symmetry.
    pub fn from_peid_style(pat: &str) -> Result<Self, PatternParseError> {
        Self::parse_sig(pat)
    }

    fn parse_sig(pat: &str) -> Result<Self, PatternParseError> {
        let mut out = Vec::new();
        for tok in pat.split_whitespace() {
            if tok.as_bytes()[0] == b'?' {
                out.push(ByteMatch::Any);
                continue;
            }
            let b = tok.as_bytes();
            if b.len() != 2 {
                return Err(PatternParseError(alloc::format!("token `{tok}`")));
            }
            let (hi, lo) = (hex_nibble(b[0]), hex_nibble(b[1]));
            match (hi, lo) {
                (Some(hi), Some(lo)) => out.push(ByteMatch::Exact(hi * 0x10 + lo)),
                _ => return Err(PatternParseError(alloc::format!("token `{tok}`"))),
            }
        }
        if out.is_empty() {
            return Err(PatternParseError(String::from("empty pattern")));
        }
        Ok(Self(out))
    }

    /// Builds a pattern from a raw byte template and a mask, where each mask
    /// char is `x` (match `pat[i]` exactly) or `?` (wildcard). `pat` and `mask`
    /// must be the same length.
    pub fn from_code_style(pat: &[u8], mask: &str) -> Result<Self, PatternParseError> {
        let mask = mask.as_bytes();
        if pat.len() != mask.len() {
            return Err(PatternParseError(String::from("pat/mask length mismatch")));
        }
        let mut out = Vec::with_capacity(pat.len());
        for (i, &m) in mask.iter().enumerate() {
            match m {
                b'x' | b'X' => out.push(ByteMatch::Exact(pat[i])),
                b'?' => out.push(ByteMatch::Any),
                _ => return Err(PatternParseError(alloc::format!("mask char `{}`", m as char))),
            }
        }
        Ok(Self(out))
    }
}

impl Matcher for DynPattern {
    fn matches(&self, seq: &[u8]) -> bool {
        seq.len() == self.0.len() && self.0.iter().zip(seq.iter()).all(|(m, &b)| m.matches(b))
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ida_and_peid() {
        let data = b"\x11\x22\x33";
        assert!(DynPattern::from_ida_style("11 ? 33").unwrap().matches(data));
        assert!(DynPattern::from_peid_style("11 ?? 33").unwrap().matches(data));
        assert!(!DynPattern::from_ida_style("11 22 44").unwrap().matches(data));
    }

    #[test]
    fn code_style() {
        let pat = DynPattern::from_code_style(b"\x11\x55\xE2", "x?x").unwrap();
        assert!(pat.matches(b"\x11\x01\xE2"));
        assert!(!pat.matches(b"\x11\x01\xE3"));
        assert_eq!(pat.len(), 3);
    }

    #[test]
    fn errors() {
        assert!(DynPattern::from_ida_style("").is_err());
        assert!(DynPattern::from_ida_style("11 2 33").is_err());
        assert!(DynPattern::from_ida_style("11 ZZ").is_err());
        assert!(DynPattern::from_code_style(b"\x11", "xx").is_err());
    }
}
