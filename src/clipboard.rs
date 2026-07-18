//! Copy & paste support. Copies write RON-serialized [`ClipboardPayload`]s via egui's
//! `Context::copy_text`; menu-triggered pastes read the system clipboard on demand through
//! `arboard` (egui only exposes clipboard reads via the `Event::Paste` keyboard event).

use crate::{address::parse_address, field::FieldKind, project::DataField};
use eframe::egui::Context;
use serde::{Deserialize, Serialize};

/// A structured clipboard payload. Serialized as RON, mirroring [`crate::project::ProjectData`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipboardPayload {
    /// One or more field definitions (ReClass-style structure copy).
    Fields(Vec<DataField>),
    /// A raw memory value read from a field, tagged with its kind.
    Value { kind: FieldKind, bytes: Vec<u8> },
    /// A memory address.
    Address(usize),
}

/// The result of interpreting pasted clipboard text.
pub enum ParsedPaste {
    /// A structured YClass payload.
    Payload(ClipboardPayload),
    /// Raw hex text that parsed as an address (least-destructive fallback).
    Address(usize),
    /// Nothing we can act on.
    None,
}

/// Writes a payload to the clipboard.
pub fn write(ctx: &Context, payload: &ClipboardPayload) {
    if let Ok(text) = ron::to_string(payload) {
        ctx.copy_text(text);
    }
}

/// Reads the current clipboard text on demand (for menu-triggered pastes).
pub fn read() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// Interprets pasted text. Structured RON payloads are authoritative; otherwise a bare hex string
/// is treated as an address (never as a memory write) to avoid accidental memory corruption.
pub fn parse(text: &str) -> ParsedPaste {
    if let Ok(payload) = ron::from_str::<ClipboardPayload>(text) {
        ParsedPaste::Payload(payload)
    } else if let Some(addr) = parse_address(text.trim()) {
        ParsedPaste::Address(addr)
    } else {
        ParsedPaste::None
    }
}
