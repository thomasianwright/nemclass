//! Address arithmetic: pointer chains, RIP-relative resolution, and a small
//! address-expression evaluator.

use crate::error::{Result, SdkError};
use crate::target::Target;

/// Follows a multi-level pointer chain. Every offset but the last is added to
/// the running pointer and dereferenced (width-aware); the final offset is added
/// without a dereference. `offsets` empty returns `base`.
///
/// ```text
/// resolve(base, [a, b, c]) = *(*(base + a) + b) + c
/// ```
pub fn resolve(target: &Target, base: usize, offsets: &[usize]) -> Option<usize> {
    let Some((last, chain)) = offsets.split_last() else {
        return Some(base);
    };
    let mut cur = base;
    for &off in chain {
        cur = target.read_ptr(cur.wrapping_add(off))?;
    }
    Some(cur.wrapping_add(*last))
}

/// Resolves a RIP-relative reference: reads the signed 32-bit displacement at
/// `rel32_addr` and adds it to `next_insn_addr` (the address of the instruction
/// following the displacement). Returns the absolute target address.
pub fn rip_relative(target: &Target, rel32_addr: usize, next_insn_addr: usize) -> Option<usize> {
    let rel = target.read_pod::<i32>(rel32_addr)?;
    Some((next_insn_addr as i64).wrapping_add(rel as i64) as usize)
}

/// Environment an address expression is evaluated against: module base lookup
/// and pointer dereference. Implemented by [`Target`]; abstracted so the
/// evaluator can be unit-tested without a live process.
pub trait AddrEnv {
    /// Returns the base address of the named module.
    fn module_base(&self, name: &str) -> Result<usize>;
    /// Reads a pointer at `address` (width-aware), or `None` if unreadable.
    fn deref(&self, address: usize) -> Option<usize>;
}

impl AddrEnv for Target {
    fn module_base(&self, name: &str) -> Result<usize> {
        Ok(self.module(name)?.base)
    }
    fn deref(&self, address: usize) -> Option<usize> {
        self.read_ptr(address)
    }
}

/// Evaluates an address expression against `env`.
///
/// Grammar (all bare numbers are hexadecimal, matching yclass convention):
/// - `0x1234` / `1234` — a number
/// - `<module.exe>` — the module's base address
/// - `[expr]` — dereference (read a pointer, width-aware)
/// - `(expr)` — grouping
/// - `+` `-` `*` — arithmetic (wrapping)
///
/// e.g. `"[<game.exe>+0x1A2B]+0x10"`.
pub fn eval(env: &impl AddrEnv, expr: &str) -> Result<usize> {
    let mut p = Parser {
        src: expr.as_bytes(),
        pos: 0,
        env,
    };
    let v = p.expr()?;
    p.skip_ws();
    if p.pos != p.src.len() {
        return Err(SdkError::Expr(format!(
            "unexpected trailing input at byte {}",
            p.pos
        )));
    }
    Ok(v)
}

struct Parser<'a, E: AddrEnv> {
    src: &'a [u8],
    pos: usize,
    env: &'a E,
}

impl<E: AddrEnv> Parser<'_, E> {
    fn skip_ws(&mut self) {
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.src.get(self.pos).copied()
    }

    fn expr(&mut self) -> Result<usize> {
        let mut acc = self.term()?;
        while let Some(op) = self.peek() {
            match op {
                b'+' => {
                    self.pos += 1;
                    acc = acc.wrapping_add(self.term()?);
                }
                b'-' => {
                    self.pos += 1;
                    acc = acc.wrapping_sub(self.term()?);
                }
                _ => break,
            }
        }
        Ok(acc)
    }

    fn term(&mut self) -> Result<usize> {
        let mut acc = self.factor()?;
        while let Some(b'*') = self.peek() {
            self.pos += 1;
            acc = acc.wrapping_mul(self.factor()?);
        }
        Ok(acc)
    }

    fn factor(&mut self) -> Result<usize> {
        match self.peek() {
            Some(b'[') => {
                self.pos += 1;
                let inner = self.expr()?;
                self.expect(b']')?;
                self.env
                    .deref(inner)
                    .ok_or(SdkError::Access { address: inner })
            }
            Some(b'(') => {
                self.pos += 1;
                let inner = self.expr()?;
                self.expect(b')')?;
                Ok(inner)
            }
            // Module base, either quoted (`"game.exe"`) or angle-bracketed (`<game.exe>`).
            Some(b'"') => self.module_ref(b'"'),
            Some(b'<') => self.module_ref(b'>'),
            Some(c) if c.is_ascii_hexdigit() => self.number(),
            Some(c) => Err(SdkError::Expr(format!(
                "unexpected `{}` at byte {}",
                c as char, self.pos
            ))),
            None => Err(SdkError::Expr("unexpected end of expression".into())),
        }
    }

    /// Reads a module name terminated by `close` (the opening delimiter has
    /// already been consumed) and resolves it to the module's base address.
    fn module_ref(&mut self, close: u8) -> Result<usize> {
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.src.len() && self.src[self.pos] != close {
            self.pos += 1;
        }
        let name = std::str::from_utf8(&self.src[start..self.pos])
            .map_err(|_| SdkError::Expr("invalid module name".into()))?
            .to_owned();
        self.expect(close)?;
        self.env.module_base(&name)
    }

    fn number(&mut self) -> Result<usize> {
        self.skip_ws();
        // Optional 0x prefix; digits are always hex.
        if self.src[self.pos..].starts_with(b"0x") || self.src[self.pos..].starts_with(b"0X") {
            self.pos += 2;
        }
        let start = self.pos;
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_hexdigit() {
            self.pos += 1;
        }
        if self.pos == start {
            return Err(SdkError::Expr("expected a number".into()));
        }
        let text = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
        usize::from_str_radix(text, 16).map_err(|e| SdkError::Expr(e.to_string()))
    }

    fn expect(&mut self, c: u8) -> Result<()> {
        if self.peek() == Some(c) {
            self.pos += 1;
            Ok(())
        } else {
            Err(SdkError::Expr(format!("expected `{}`", c as char)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A mock environment: fixed module bases and a fixed pointer map.
    struct MockEnv {
        modules: HashMap<String, usize>,
        memory: HashMap<usize, usize>,
    }

    impl AddrEnv for MockEnv {
        fn module_base(&self, name: &str) -> Result<usize> {
            self.modules
                .get(name)
                .copied()
                .ok_or_else(|| SdkError::ModuleNotFound(name.into()))
        }
        fn deref(&self, address: usize) -> Option<usize> {
            self.memory.get(&address).copied()
        }
    }

    fn env() -> MockEnv {
        let mut modules = HashMap::new();
        modules.insert("game.exe".to_string(), 0x1_0000);
        let mut memory = HashMap::new();
        memory.insert(0x1_0010, 0xDEAD_0000); // *(base+0x10)
        memory.insert(0xDEAD_0008, 0xBEEF_0000); // *(that+8)
        MockEnv { modules, memory }
    }

    #[test]
    fn numbers_and_arithmetic() {
        let e = env();
        assert_eq!(eval(&e, "0x10+0x20").unwrap(), 0x30);
        assert_eq!(eval(&e, "100-1").unwrap(), 0xFF);
        assert_eq!(eval(&e, "2*8+1").unwrap(), 0x11);
        assert_eq!(eval(&e, "(2+3)*4").unwrap(), 0x14);
    }

    #[test]
    fn module_and_deref() {
        let e = env();
        // Angle-bracket form.
        assert_eq!(eval(&e, "<game.exe>").unwrap(), 0x1_0000);
        assert_eq!(eval(&e, "<game.exe>+0x10").unwrap(), 0x1_0010);
        assert_eq!(eval(&e, "[<game.exe>+0x10]").unwrap(), 0xDEAD_0000);
        assert_eq!(eval(&e, "[[<game.exe>+0x10]+8]").unwrap(), 0xBEEF_0000);
        // Quoted form (equivalent).
        assert_eq!(eval(&e, "\"game.exe\"+0x10").unwrap(), 0x1_0010);
        assert_eq!(eval(&e, "[\"game.exe\"+0x1A2B]").is_err(), true); // unmapped deref
        assert_eq!(eval(&e, "[\"game.exe\"+0x10]+0x0").unwrap(), 0xDEAD_0000);
    }

    #[test]
    fn errors() {
        let e = env();
        assert!(eval(&e, "<missing.dll>").is_err());
        assert!(eval(&e, "0x10+").is_err());
        assert!(eval(&e, "[0x999]").is_err()); // unmapped deref
        assert!(eval(&e, "0x10 0x20").is_err()); // trailing input
    }
}
