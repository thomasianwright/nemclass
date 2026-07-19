//! Cheat-table tool window: live-valued, editable, freezable address entries.
//!
//! Entries live in the shared [`crate::state::GlobalState::cheat_table`] (so the
//! scanner can add to it and it persists with the project). Frozen entries are
//! re-written every repaint tick via [`CheatTableWindow::freeze_tick`], which
//! runs whether or not the window is open.

use crate::{gui::floating_window, state::StateRef};
use eframe::egui::{ComboBox, Context, ScrollArea, TextEdit, Ui};
use nemclass_sdk::{table::FreezeValue, CheatEntry, FieldKind, Target};

/// Scalar field kinds offered when adding an entry.
const KINDS: &[FieldKind] = &[
    FieldKind::I8,
    FieldKind::U8,
    FieldKind::I16,
    FieldKind::U16,
    FieldKind::I32,
    FieldKind::U32,
    FieldKind::I64,
    FieldKind::U64,
    FieldKind::F32,
    FieldKind::F64,
    FieldKind::Bool,
];

pub struct CheatTableWindow {
    state: StateRef,
    shown: bool,
    new_desc: String,
    new_addr: String,
    new_kind: FieldKind,
    /// Per-row "value to write" scratch buffers, kept the length of the table.
    write_buf: Vec<String>,
}

impl CheatTableWindow {
    pub fn new(state: StateRef) -> Self {
        Self {
            state,
            shown: false,
            new_desc: String::new(),
            new_addr: String::new(),
            new_kind: FieldKind::I32,
            write_buf: Vec::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.shown = !self.shown;
    }

    pub fn show(&mut self, ctx: &Context) {
        // Freeze runs every frame, even while the window is closed.
        self.freeze_tick();

        let shown = self.shown;
        let response = floating_window(
            ctx,
            shown,
            "nem_cheat_viewport",
            "Cheat table",
            [600.0, 480.0],
            |ui| self.table_ui(ui),
        );

        if let Some((true, ())) = response {
            self.shown = false;
        }
    }

    /// Re-writes every frozen entry's pinned bytes to its (re-resolved) address.
    fn freeze_tick(&self) {
        let proc = self.state.borrow().process.clone();
        let guard = proc.read();
        let Some(process) = guard.as_ref() else {
            return;
        };
        let target: &Target = process;
        let st = self.state.borrow();
        for entry in st.cheat_table.all_entries() {
            if let Some(freeze) = &entry.freeze {
                if let Ok(addr) = entry.resolve(target) {
                    let _ = target.write(addr, &freeze.bytes);
                }
            }
        }
    }

    fn table_ui(&mut self, ui: &mut Ui) {
        let proc = self.state.borrow().process.clone();
        let guard = proc.read();
        let target: Option<&Target> = guard.as_ref().map(|p| &**p);
        let ptr_size = target.map(|t| t.pointer_size()).unwrap_or(8);

        // New-entry row (takes its own short state borrow when adding).
        ui.horizontal(|ui| {
            ui.add(
                TextEdit::singleline(&mut self.new_desc)
                    .hint_text("description")
                    .desired_width(110.0),
            );
            ui.add(
                TextEdit::singleline(&mut self.new_addr)
                    .hint_text("address  e.g. 0x1234 or [<game.exe>+0x10]")
                    .desired_width(220.0),
            );
            ComboBox::new("_ct_kind", "")
                .selected_text(self.new_kind.display_name().to_string())
                .show_ui(ui, |ui| {
                    for k in KINDS {
                        if ui
                            .selectable_label(self.new_kind == *k, k.display_name().to_string())
                            .clicked()
                        {
                            self.new_kind = *k;
                        }
                    }
                });
            if ui.button("Add").clicked() && !self.new_addr.trim().is_empty() {
                let desc = if self.new_desc.trim().is_empty() {
                    "entry".to_owned()
                } else {
                    self.new_desc.trim().to_owned()
                };
                self.state.borrow_mut().cheat_table.entries.push(CheatEntry::new(
                    desc,
                    self.new_addr.trim().to_owned(),
                    self.new_kind,
                ));
                self.new_desc.clear();
                self.new_addr.clear();
            }
        });
        ui.separator();

        let mut remove = None;
        let mut st = self.state.borrow_mut();
        let entries = &mut st.cheat_table.entries;
        if self.write_buf.len() != entries.len() {
            self.write_buf.resize(entries.len(), String::new());
        }

        if entries.is_empty() {
            ui.label("No entries. Add one above, or scan a value and click “+ table”.");
        }

        ScrollArea::vertical().show(ui, |ui| {
            for (i, entry) in entries.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.add(TextEdit::singleline(&mut entry.description).desired_width(110.0));
                    ui.monospace(&entry.address);
                    let cur = target
                        .map(|t| read_field_display(t, entry, ptr_size))
                        .unwrap_or_else(|| "-".to_owned());
                    ui.monospace(cur);

                    let mut frozen = entry.freeze.is_some();
                    if ui.checkbox(&mut frozen, "freeze").changed() {
                        entry.freeze = if frozen {
                            target
                                .and_then(|t| capture_bytes(t, entry, ptr_size))
                                .map(|bytes| FreezeValue { bytes })
                        } else {
                            None
                        };
                    }

                    ui.add(
                        TextEdit::singleline(&mut self.write_buf[i])
                            .hint_text("set…")
                            .desired_width(80.0),
                    );
                    if ui.small_button("Set").clicked() {
                        if let Some(t) = target {
                            if let Some(bytes) = parse_field_value(entry.kind, &self.write_buf[i]) {
                                if let Ok(addr) = entry.resolve(t) {
                                    let _ = t.write(addr, &bytes);
                                }
                                // Keep a frozen entry pinned to the newly-set value.
                                if entry.freeze.is_some() {
                                    entry.freeze = Some(FreezeValue { bytes });
                                }
                            }
                        }
                    }
                    if ui.small_button("✕").clicked() {
                        remove = Some(i);
                    }
                });
            }
        });

        if let Some(i) = remove {
            if i < entries.len() {
                entries.remove(i);
            }
        }
    }
}

fn capture_bytes(t: &Target, entry: &CheatEntry, ptr_size: usize) -> Option<Vec<u8>> {
    let addr = entry.resolve(t).ok()?;
    t.read_bytes(addr, entry.kind.size_with_ptr(ptr_size))
}

fn read_field_display(t: &Target, entry: &CheatEntry, ptr_size: usize) -> String {
    match capture_bytes(t, entry, ptr_size) {
        Some(b) => format_field_value(entry.kind, &b),
        None => "??".to_owned(),
    }
}

fn format_field_value(kind: FieldKind, b: &[u8]) -> String {
    use FieldKind::*;
    fn a<const N: usize>(b: &[u8]) -> Option<[u8; N]> {
        b.get(..N)?.try_into().ok()
    }
    let out = match kind {
        I8 => a::<1>(b).map(|x| (x[0] as i8).to_string()),
        U8 => a::<1>(b).map(|x| x[0].to_string()),
        I16 => a::<2>(b).map(|x| i16::from_ne_bytes(x).to_string()),
        U16 => a::<2>(b).map(|x| u16::from_ne_bytes(x).to_string()),
        I32 => a::<4>(b).map(|x| i32::from_ne_bytes(x).to_string()),
        U32 => a::<4>(b).map(|x| u32::from_ne_bytes(x).to_string()),
        I64 => a::<8>(b).map(|x| i64::from_ne_bytes(x).to_string()),
        U64 => a::<8>(b).map(|x| u64::from_ne_bytes(x).to_string()),
        F32 => a::<4>(b).map(|x| format!("{:.3}", f32::from_ne_bytes(x))),
        F64 => a::<8>(b).map(|x| format!("{:.3}", f64::from_ne_bytes(x))),
        Bool => a::<1>(b).map(|x| (x[0] != 0).to_string()),
        _ => Some(
            b.iter()
                .map(|x| format!("{x:02X}"))
                .collect::<Vec<_>>()
                .join(" "),
        ),
    };
    out.unwrap_or_else(|| "??".to_owned())
}

fn parse_field_value(kind: FieldKind, text: &str) -> Option<Vec<u8>> {
    use FieldKind::*;
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    let int = || -> Option<i128> {
        if let Some(h) = t.strip_prefix("0x") {
            i128::from_str_radix(h, 16).ok()
        } else {
            t.parse::<i128>().ok()
        }
    };
    Some(match kind {
        I8 => (int()? as i8).to_ne_bytes().to_vec(),
        U8 => (int()? as u8).to_ne_bytes().to_vec(),
        I16 => (int()? as i16).to_ne_bytes().to_vec(),
        U16 => (int()? as u16).to_ne_bytes().to_vec(),
        I32 => (int()? as i32).to_ne_bytes().to_vec(),
        U32 => (int()? as u32).to_ne_bytes().to_vec(),
        I64 => (int()? as i64).to_ne_bytes().to_vec(),
        U64 => (int()? as u64).to_ne_bytes().to_vec(),
        F32 => t.parse::<f32>().ok()?.to_ne_bytes().to_vec(),
        F64 => t.parse::<f64>().ok()?.to_ne_bytes().to_vec(),
        Bool => vec![u8::from(t.eq_ignore_ascii_case("true") || t == "1")],
        _ => return None,
    })
}
