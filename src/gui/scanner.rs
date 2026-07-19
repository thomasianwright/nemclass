//! Memory value scanner tool window (CheatEngine-style first/next scan).
//!
//! Runs a [`Scanner`] over the attached process and lets the user refine matches
//! and push a hit into the shared cheat table ([`crate::state::GlobalState`]).

use crate::{gui::floating_window, state::StateRef};
use eframe::egui::{ComboBox, Context, ScrollArea, TextEdit, Ui};
use nemclass_sdk::{
    CheatEntry, FieldKind, ScanCompare, ScanConfig, ScanResults, ScanType, ScanValue, Scanner,
    Target,
};

/// Value types offered in the scanner combo.
const SCAN_TYPES: &[(&str, ScanType)] = &[
    ("i32", ScanType::I32),
    ("u32", ScanType::U32),
    ("f32", ScanType::F32),
    ("f64", ScanType::F64),
    ("i64", ScanType::I64),
    ("u64", ScanType::U64),
    ("i16", ScanType::I16),
    ("u16", ScanType::U16),
    ("i8", ScanType::I8),
    ("u8", ScanType::U8),
];

/// Most result rows rendered (with live values) per frame.
const MAX_SHOWN: usize = 200;

pub struct ScannerWindow {
    state: StateRef,
    shown: bool,
    ty: ScanType,
    value: String,
    results: Option<ScanResults>,
}

impl ScannerWindow {
    pub fn new(state: StateRef) -> Self {
        Self {
            state,
            shown: false,
            ty: ScanType::I32,
            value: String::new(),
            results: None,
        }
    }

    pub fn toggle(&mut self) {
        self.shown = !self.shown;
    }

    fn type_label(ty: ScanType) -> &'static str {
        SCAN_TYPES
            .iter()
            .find(|(_, t)| *t == ty)
            .map(|(l, _)| *l)
            .unwrap_or("i32")
    }

    pub fn show(&mut self, ctx: &Context) {
        let shown = self.shown;
        let response = floating_window(
            ctx,
            shown,
            "nem_scanner_viewport",
            "Memory scanner",
            [440.0, 480.0],
            |ui| {
                ui.horizontal(|ui| {
                    ComboBox::new("_scan_ty", "Value type")
                        .selected_text(Self::type_label(self.ty))
                        .show_ui(ui, |ui| {
                            for (label, t) in SCAN_TYPES {
                                if ui.selectable_label(self.ty == *t, *label).clicked() {
                                    self.ty = *t;
                                }
                            }
                        });
                    ui.label("Value:");
                    ui.add(TextEdit::singleline(&mut self.value).desired_width(140.0));
                });

                ui.horizontal(|ui| {
                    if ui.button("First scan").clicked() {
                        match parse_value(self.ty, &self.value) {
                            Some(v) => self.run_first(ScanCompare::Exact(v)),
                            None => self.warn("Enter a valid value for the chosen type"),
                        }
                    }
                    if ui.button("Unknown initial").clicked() {
                        self.run_first(ScanCompare::Unknown);
                    }
                    if ui.button("Reset").clicked() {
                        self.results = None;
                    }
                });

                if self.results.is_some() {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Next scan:");
                        if ui.button("= value").clicked() {
                            match parse_value(self.ty, &self.value) {
                                Some(v) => self.run_next(ScanCompare::Exact(v)),
                                None => self.warn("Enter a valid value"),
                            }
                        }
                        if ui.button("Increased").clicked() {
                            self.run_next(ScanCompare::Increased);
                        }
                        if ui.button("Decreased").clicked() {
                            self.run_next(ScanCompare::Decreased);
                        }
                        if ui.button("Changed").clicked() {
                            self.run_next(ScanCompare::Changed);
                        }
                        if ui.button("Unchanged").clicked() {
                            self.run_next(ScanCompare::Unchanged);
                        }
                    });
                }

                ui.separator();
                self.results_ui(ui);
            },
        );

        if let Some((true, ())) = response {
            self.shown = false;
        }
    }

    fn run_first(&mut self, cmp: ScanCompare) {
        let ty = self.ty;
        let proc = self.state.borrow().process.clone();
        let outcome = {
            let guard = proc.read();
            match guard.as_ref() {
                Some(process) => Scanner::new(&**process, ScanConfig::new(ty))
                    .first_scan(cmp)
                    .map_err(|e| e.to_string()),
                None => Err("Not attached to a process".to_owned()),
            }
        };
        match outcome {
            Ok(r) => {
                let n = r.len();
                self.results = Some(r);
                self.info(format!("{n} result(s)"));
            }
            Err(msg) => self.warn(msg),
        }
    }

    fn run_next(&mut self, cmp: ScanCompare) {
        let ty = self.ty;
        let Some(prev) = self.results.take() else {
            return;
        };
        let proc = self.state.borrow().process.clone();
        let outcome = {
            let guard = proc.read();
            match guard.as_ref() {
                Some(process) => Scanner::new(&**process, ScanConfig::new(ty))
                    .next_scan(&prev, cmp)
                    .map_err(|e| e.to_string()),
                None => Err("Not attached to a process".to_owned()),
            }
        };
        match outcome {
            Ok(r) => {
                let n = r.len();
                self.results = Some(r);
                self.info(format!("{n} result(s)"));
            }
            Err(msg) => {
                self.results = Some(prev);
                self.warn(msg);
            }
        }
    }

    fn results_ui(&mut self, ui: &mut Ui) {
        // Snapshot the visible rows (with live values) up front, so no borrow of
        // `self`/the process outlives the draw.
        let (ty, total, rows) = {
            let Some(results) = self.results.as_ref() else {
                ui.label("No scan yet. Pick a type, enter a value, and click First scan.");
                return;
            };
            let ty = results.value_type();
            let total = results.len();
            let proc = self.state.borrow().process.clone();
            let guard = proc.read();
            let target: Option<&Target> = guard.as_ref().map(|p| &**p);
            let rows: Vec<(usize, String)> = results
                .addresses()
                .iter()
                .take(MAX_SHOWN)
                .map(|&addr| {
                    let val = target
                        .map(|t| read_display(t, addr, ty))
                        .unwrap_or_else(|| "-".to_owned());
                    (addr, val)
                })
                .collect();
            (ty, total, rows)
        };

        ui.label(format!("{total} result(s)"));
        let mut add = None;
        ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for (addr, val) in &rows {
                ui.horizontal(|ui| {
                    ui.monospace(format!("{addr:#016x}"));
                    ui.monospace(val);
                    if ui.small_button("+ table").clicked() {
                        add = Some(*addr);
                    }
                });
            }
            if total > rows.len() {
                ui.label(format!("… {} more not shown", total - rows.len()));
            }
        });

        if let Some(addr) = add {
            self.add_to_table(addr, ty);
        }
    }

    fn add_to_table(&mut self, addr: usize, ty: ScanType) {
        let kind = ty.to_field_kind().unwrap_or(FieldKind::U32);
        let mut st = self.state.borrow_mut();
        st.cheat_table.entries.push(CheatEntry::new(
            format!("Scan {addr:#x}"),
            format!("{addr:#x}"),
            kind,
        ));
        st.toasts.info("Added to cheat table");
    }

    fn warn(&self, msg: impl Into<String>) {
        self.state.borrow_mut().toasts.warning(msg.into());
    }

    fn info(&self, msg: impl Into<String>) {
        self.state.borrow_mut().toasts.info(msg.into());
    }
}

/// Parses the user's text into a [`ScanValue`] for `ty` (hex `0x…` allowed for ints).
fn parse_value(ty: ScanType, text: &str) -> Option<ScanValue> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    if ty.is_float() {
        t.parse::<f64>().ok().map(ScanValue::Float)
    } else if let Some(h) = t.strip_prefix("0x") {
        i128::from_str_radix(h, 16).ok().map(ScanValue::Int)
    } else {
        t.parse::<i128>().ok().map(ScanValue::Int)
    }
}

fn read_display(target: &Target, addr: usize, ty: ScanType) -> String {
    match target.read_bytes(addr, ty.size()) {
        Some(bytes) => format_scan_value(ty, &bytes),
        None => "??".to_owned(),
    }
}

fn format_scan_value(ty: ScanType, b: &[u8]) -> String {
    fn a<const N: usize>(b: &[u8]) -> Option<[u8; N]> {
        b.get(..N)?.try_into().ok()
    }
    let out = match ty {
        ScanType::I8 => a::<1>(b).map(|x| (x[0] as i8).to_string()),
        ScanType::U8 => a::<1>(b).map(|x| x[0].to_string()),
        ScanType::I16 => a::<2>(b).map(|x| i16::from_ne_bytes(x).to_string()),
        ScanType::U16 => a::<2>(b).map(|x| u16::from_ne_bytes(x).to_string()),
        ScanType::I32 => a::<4>(b).map(|x| i32::from_ne_bytes(x).to_string()),
        ScanType::U32 => a::<4>(b).map(|x| u32::from_ne_bytes(x).to_string()),
        ScanType::I64 => a::<8>(b).map(|x| i64::from_ne_bytes(x).to_string()),
        ScanType::U64 => a::<8>(b).map(|x| u64::from_ne_bytes(x).to_string()),
        ScanType::F32 => a::<4>(b).map(|x| format!("{:.3}", f32::from_ne_bytes(x))),
        ScanType::F64 => a::<8>(b).map(|x| format!("{:.3}", f64::from_ne_bytes(x))),
        ScanType::Bytes => None,
    };
    out.unwrap_or_else(|| "??".to_owned())
}
