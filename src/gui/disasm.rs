//! Disassembly tool window.
//!
//! Combines the SDK [`disasm`](nemclass_sdk::disasm) analysis with the shared
//! debugger state:
//! - **Memory map** (left): every `/proc/<pid>/maps` region incl. anonymous
//!   mappings, classified; click to disassemble from its start.
//! - **Disassembly** (center): instructions with a breakpoint gutter (toggles
//!   [`GlobalState::breakpoints`], applied by the debugger window), the current
//!   RIP highlighted, and click-to-follow on call/branch targets.
//! - **Strings / Calls / Functions** (bottom): per-region analysis; click to
//!   navigate.

use crate::{gui::floating_window, state::StateRef};
use eframe::{
    egui::{
        self, CentralPanel, Context, RichText, ScrollArea, SidePanel, TextEdit, TopBottomPanel, Ui,
    },
    epaint::Color32,
};
use nemclass_sdk::{
    call_targets, disassemble, disassemble_range, find_functions, find_strings, memory_map,
    FlowKind, Insn, MapRegion, RegionKind, StringHit, Target,
};

/// Instructions decoded for the center view each frame.
const ROWS: usize = 200;
/// Cap on analysis entries listed in the bottom tabs.
const LIST_CAP: usize = 1000;

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Strings,
    Calls,
    Functions,
}

pub struct DisasmWindow {
    state: StateRef,
    shown: bool,
    regions: Vec<MapRegion>,
    selected: Option<usize>,
    cursor: usize,
    goto: String,
    tab: Tab,
    analyzed: Option<usize>,
    strings: Vec<StringHit>,
    calls: Vec<usize>,
    functions: Vec<usize>,
}

impl DisasmWindow {
    pub fn new(state: StateRef) -> Self {
        Self {
            state,
            shown: false,
            regions: Vec::new(),
            selected: None,
            cursor: 0,
            goto: String::new(),
            tab: Tab::Strings,
            analyzed: None,
            strings: Vec::new(),
            calls: Vec::new(),
            functions: Vec::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.shown = !self.shown;
    }

    pub fn show(&mut self, ctx: &Context) {
        let shown = self.shown;
        let response = floating_window(
            ctx,
            shown,
            "nem_disasm_viewport",
            "Disassembly",
            [860.0, 620.0],
            |ui| self.ui(ui),
        );
        if let Some((true, ())) = response {
            self.shown = false;
        }
    }

    fn region_of(&self, addr: usize) -> Option<usize> {
        self.regions
            .iter()
            .position(|r| addr >= r.from && addr < r.to)
    }

    fn analyze(&mut self, target: &Target, idx: usize) {
        let Some(region) = self.regions.get(idx).cloned() else {
            return;
        };
        self.strings = find_strings(target, region.from, region.size(), 4);
        if region.exec {
            let insns = disassemble_range(target, region.from, region.size());
            self.calls = call_targets(&insns);
            self.functions = find_functions(target, region.from, region.size());
        } else {
            self.calls.clear();
            self.functions.clear();
        }
        self.analyzed = Some(idx);
    }

    fn ui(&mut self, ui: &mut Ui) {
        let proc = self.state.borrow().process.clone();
        let guard = proc.read();
        let target: Option<&Target> = guard.as_ref().map(|p| &**p);
        let pid = target.map(|t| t.id());
        let rip = self.state.borrow().debug_rip;
        let bps = self.state.borrow().breakpoints.clone();

        if target.is_none() {
            ui.label("Attach to a process to disassemble.");
            return;
        }
        // First open with an attached process: load the map and start somewhere.
        if self.regions.is_empty() {
            if let Some(pid) = pid {
                self.regions = memory_map(pid).unwrap_or_default();
            }
            if self.cursor == 0 {
                if let Some(i) = self.regions.iter().position(|r| r.exec) {
                    self.selected = Some(i);
                    self.cursor = self.regions[i].from;
                }
            }
        }

        let mut refresh = false;
        let mut follow = false;
        let mut goto_go = false;
        let mut select = None;
        let mut navigate = None;
        let mut toggle_bp = None;

        ui.horizontal(|ui| {
            ui.label("Goto:");
            ui.add(TextEdit::singleline(&mut self.goto).desired_width(140.0));
            goto_go = ui.button("Go").clicked();
            refresh = ui.button("Refresh map").clicked();
            follow = ui.add_enabled(rip.is_some(), egui::Button::new("Follow RIP")).clicked();
            if let Some(rip) = rip {
                ui.label(format!("rip={rip:#x}"));
            }
        });
        ui.separator();

        // Left: memory map (incl. anonymous regions).
        SidePanel::left("_dism_map")
            .resizable(true)
            .default_width(240.0)
            .show_inside(ui, |ui| {
                ui.label(RichText::new("Memory map").strong());
                ScrollArea::vertical().id_salt("_map").show(ui, |ui| {
                    for (i, r) in self.regions.iter().enumerate() {
                        let perms = format!(
                            "{}{}{}",
                            if r.read { "r" } else { "-" },
                            if r.write { "w" } else { "-" },
                            if r.exec { "x" } else { "-" },
                        );
                        let label = format!("{perms} {} {}", kind_tag(r.kind), r.label());
                        if ui.selectable_label(self.selected == Some(i), label).clicked() {
                            select = Some(i);
                        }
                    }
                });
            });

        // Bottom: strings / calls / functions.
        TopBottomPanel::bottom("_dism_tabs")
            .resizable(true)
            .default_height(180.0)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    for (t, label) in [
                        (Tab::Strings, "Strings"),
                        (Tab::Calls, "Calls"),
                        (Tab::Functions, "Functions"),
                    ] {
                        if ui.selectable_label(self.tab == t, label).clicked() {
                            self.tab = t;
                        }
                    }
                });
                ScrollArea::vertical().id_salt("_tabs").show(ui, |ui| match self.tab {
                    Tab::Strings => {
                        for s in self.strings.iter().take(LIST_CAP) {
                            if ui
                                .button(format!("{:#x}  {}", s.addr, truncate(&s.text, 70)))
                                .clicked()
                            {
                                navigate = Some(s.addr);
                            }
                        }
                    }
                    Tab::Calls => {
                        for &c in self.calls.iter().take(LIST_CAP) {
                            if ui.button(format!("call → {c:#x}")).clicked() {
                                navigate = Some(c);
                            }
                        }
                    }
                    Tab::Functions => {
                        for &f in self.functions.iter().take(LIST_CAP) {
                            if ui.button(format!("fn {f:#x}")).clicked() {
                                navigate = Some(f);
                            }
                        }
                    }
                });
            });

        // Center: disassembly.
        let view: Vec<Insn> = target
            .map(|t| disassemble(t, self.cursor, ROWS))
            .unwrap_or_default();
        CentralPanel::default().show_inside(ui, |ui| {
            ScrollArea::vertical().id_salt("_disasm").show(ui, |ui| {
                for insn in &view {
                    ui.horizontal(|ui| {
                        // Breakpoint gutter.
                        let is_bp = bps.contains(&insn.addr);
                        let dot = if is_bp {
                            RichText::new("●").color(Color32::from_rgb(0xE0, 0x40, 0x40))
                        } else {
                            RichText::new("○").color(Color32::DARK_GRAY)
                        };
                        if ui.add(egui::Button::new(dot).frame(false)).clicked() {
                            toggle_bp = Some(insn.addr);
                        }

                        // Current-instruction marker.
                        let at_rip = rip == Some(insn.addr);
                        let addr_txt = RichText::new(format!("{:>#018x}", insn.addr)).monospace();
                        ui.label(if at_rip {
                            RichText::new(format!("{:>#018x}", insn.addr))
                                .monospace()
                                .color(Color32::from_rgb(0x60, 0xC0, 0xFF))
                        } else {
                            addr_txt
                        });

                        ui.label(RichText::new(hex_bytes(&insn.bytes)).monospace().weak());
                        ui.monospace(&insn.text);

                        if let Some(tgt) = insn.target {
                            if matches!(insn.kind, FlowKind::Call | FlowKind::Jump | FlowKind::CondJump)
                                && ui.small_button("→").on_hover_text(format!("{tgt:#x}")).clicked()
                            {
                                navigate = Some(tgt);
                            }
                        }
                    });
                }
            });
        });

        // --- apply intents (guard/target still alive) ---
        if refresh {
            if let Some(pid) = pid {
                self.regions = memory_map(pid).unwrap_or_default();
                self.analyzed = None;
            }
        }
        if let Some(i) = select {
            self.selected = Some(i);
            self.cursor = self.regions[i].from;
        }
        if goto_go {
            if let Some(a) = parse_addr(&self.goto) {
                navigate = Some(a);
            }
        }
        if follow {
            if let Some(a) = rip {
                navigate = Some(a);
            }
        }
        if let Some(addr) = navigate {
            self.cursor = addr;
            self.selected = self.region_of(addr);
        }
        if let Some(addr) = toggle_bp {
            let mut st = self.state.borrow_mut();
            if st.breakpoints.contains(&addr) {
                st.breakpoints.retain(|&a| a != addr);
            } else {
                st.breakpoints.push(addr);
            }
        }
        // Lazily (re)analyze the selected region.
        if let (Some(t), Some(idx)) = (target, self.selected) {
            if self.analyzed != Some(idx) {
                self.analyze(t, idx);
            }
        }
    }
}

fn kind_tag(kind: RegionKind) -> &'static str {
    match kind {
        RegionKind::Module => "[mod]",
        RegionKind::Heap => "[heap]",
        RegionKind::Stack => "[stack]",
        RegionKind::Vdso => "[vdso]",
        RegionKind::Anon => "[anon]",
        RegionKind::Other => "[misc]",
    }
}

fn hex_bytes(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!("{}…", s.chars().take(n).collect::<String>())
    } else {
        s.to_owned()
    }
}

fn parse_addr(s: &str) -> Option<usize> {
    let t = s.trim();
    usize::from_str_radix(t.strip_prefix("0x").unwrap_or(t), 16).ok()
}
