mod r#static;
pub use r#static::*;

#[cfg(feature = "alloc")]
mod dynamic;
#[cfg(feature = "alloc")]
pub use dynamic::*;

/// A single position in a memory pattern: either an exact byte or a wildcard
/// that matches any byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ByteMatch {
    /// Matches only this exact byte.
    Exact(u8),
    /// Matches any byte (wildcard, `?`/`??` in IDA/PEID syntax).
    Any,
}

impl ByteMatch {
    /// Returns `true` if `byte` satisfies this position (always `true` for
    /// [`ByteMatch::Any`]).
    #[inline]
    pub const fn matches(self, byte: u8) -> bool {
        match self {
            ByteMatch::Exact(b) => b == byte,
            ByteMatch::Any => true,
        }
    }
}

/// Trait for generalizing static & dynamic memory patterns.
#[allow(clippy::len_without_is_empty)]
pub trait Matcher {
    /// Matches byte sequence agains the pattern
    fn matches(&self, seq: &[u8]) -> bool;

    /// Size of the pattern
    fn len(&self) -> usize;
}

impl<'a> Matcher for &'a [u8] {
    fn matches(&self, seq: &[u8]) -> bool {
        seq.len() == self.len() && self.iter().zip(seq.iter()).all(|(a, b)| a.eq(b))
    }

    fn len(&self) -> usize {
        (*self as &[u8]).len()
    }
}
