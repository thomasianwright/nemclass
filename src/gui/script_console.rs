//! A Lua scripting console window. Runs scripts through the headless
//! [`ScriptEngine`], captures `print` output, exposes the attached process id as
//! the `PID` global, and — if a script sets the `EXPORT` global to a project RON
//! string — merges the declared classes into the GUI, updating same-named
//! classes in place and adding new ones while keeping the rest (undoably).
//!
//! Scripts are loaded from and saved to the open project's `scripts/` folder, so
//! they can be version-controlled alongside the project.

use super::completion::{complete, Completion};
use crate::{gui::floating_window, project::ProjectData, state::StateRef};
use eframe::egui::{
    text::{CCursor, CCursorRange},
    text_edit::{TextEditOutput, TextEditState},
    Button, CentralPanel, Context, FontId, Frame, Id, ScrollArea, SidePanel, TextEdit,
};
use nemclass_scripting::{ClassHost, ScriptEngine};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Persistent id of the code editor's `TextEdit`, needed to move the caret after
/// inserting a completion.
const EDITOR_ID: &str = "nem_script_editor";

/// Bridges scripts to the GUI's live class list, so `nem.set_class_address` and
/// friends can drive the inspector.
#[derive(Clone)]
struct GuiClassHost {
    state: StateRef,
    /// Addresses set for classes that don't exist yet — typically ones the same
    /// script creates via `EXPORT`, which is only imported after the run. These
    /// are applied once the import has created the classes (see [`Self::import`]).
    /// `Rc` so every clone the engine makes shares one list.
    deferred: Rc<RefCell<Vec<(String, usize)>>>,
}

impl GuiClassHost {
    fn new(state: StateRef) -> Self {
        Self {
            state,
            deferred: Rc::new(RefCell::new(Vec::new())),
        }
    }
}

impl ClassHost for GuiClassHost {
    fn class_names(&self) -> Vec<String> {
        self.state
            .try_borrow()
            .map(|s| s.class_list.classes().iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default()
    }

    fn set_class_address(&self, name: &str, address: usize) -> bool {
        if let Ok(s) = self.state.try_borrow() {
            if let Some(class) = s.class_list.by_name(name) {
                class.address.set(address);
                return true;
            }
        }
        // The class doesn't exist yet (e.g. it's created by an `EXPORT` later in
        // this same run). Remember the address instead of failing the script.
        self.deferred.borrow_mut().push((name.to_owned(), address));
        true
    }

    fn class_address(&self, name: &str) -> Option<usize> {
        self.state
            .try_borrow()
            .ok()
            .and_then(|s| s.class_list.by_name(name).map(|c| c.address.get()))
    }
}

const DEFAULT_SCRIPT: &str = r#"-- nem.* scripting API. `PID` is the attached process id (or nil).
-- Set EXPORT to a project RON string to merge classes into the GUI
-- (same-named classes are updated in place; new ones are added).

local c = nem.class("Example")
c:field("health", nem.kinds.i32)
c:field("pos", nem.kinds.vec(3, "f32"))
c:field("name", nem.kinds.strptr)

local p = nem.project()
p:add(c:build())
print(p:generate("rust"))
EXPORT = p:to_ron()
"#;

pub struct ScriptConsole {
    state: StateRef,
    shown: bool,
    script: String,
    script_name: String,
    output: String,
}

impl ScriptConsole {
    pub fn new(state: StateRef) -> Self {
        Self {
            state,
            shown: false,
            script: DEFAULT_SCRIPT.to_owned(),
            script_name: "scratch.lua".to_owned(),
            output: String::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.shown = !self.shown;
    }

    pub fn show(&mut self, ctx: &Context) {
        if !self.shown {
            return;
        }

        let scripts_dir = self.state.borrow().scripts_dir();
        let scripts = scripts_dir.as_deref().map(list_lua).unwrap_or_default();
        let editor_id = Id::new(EDITOR_ID);

        // Collect actions inside the closure; apply them after (avoids borrow clashes).
        let mut run = false;
        let mut new = false;
        let mut save = false;
        let mut delete = false;
        let mut load: Option<PathBuf> = None;
        let mut accept: Option<(Range<usize>, String)> = None;

        let response = floating_window(
            ctx,
            true,
            "nem_script_editor_viewport",
            "Script editor",
            [760.0, 540.0],
            |ui| {
                // Toolbar.
                ui.horizontal(|ui| {
                    new = ui.button("New").clicked();
                    let can_save = scripts_dir.is_some() && !self.script_name.trim().is_empty();
                    save = ui.add_enabled(can_save, Button::new("Save")).clicked();
                    run = ui.button("Run").clicked();
                    ui.separator();
                    ui.label("File:");
                    ui.add(TextEdit::singleline(&mut self.script_name).desired_width(160.0));
                    delete = ui.add_enabled(scripts_dir.is_some(), Button::new("Delete")).clicked();
                });
                ui.separator();

                // Explorer.
                SidePanel::left("nem_script_explorer")
                    .resizable(true)
                    .default_width(150.0)
                    .show_inside(ui, |ui| {
                        ui.strong("Scripts");
                        ui.separator();
                        ScrollArea::vertical().show(ui, |ui| {
                            if scripts.is_empty() {
                                ui.weak("(none yet — Save to create)");
                            }
                            for path in &scripts {
                                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                    let selected = name == self.script_name;
                                    if ui.selectable_label(selected, name).clicked() {
                                        load = Some(path.clone());
                                    }
                                }
                            }
                        });
                    });

                // Editor + output.
                CentralPanel::default().show_inside(ui, |ui| {
                    let output = TextEdit::multiline(&mut self.script)
                        .id(editor_id)
                        .code_editor()
                        .desired_rows(18)
                        .desired_width(f32::INFINITY)
                        .font(FontId::monospace(12.))
                        .show(ui);

                    // IntelliSense: suggestions for the token under the caret.
                    if output.response.has_focus() {
                        if let Some(caret) = caret_index(&output) {
                            if let Some(comp) = complete(&self.script, caret) {
                                accept = render_completions(ui, &comp);
                            }
                        }
                    }

                    ui.separator();
                    ui.label("Output:");
                    ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                        ui.add(
                            TextEdit::multiline(&mut self.output.as_str())
                                .desired_width(f32::INFINITY)
                                .font(FontId::monospace(12.)),
                        );
                    });
                });
            },
        );

        if let Some((true, ())) = response {
            self.shown = false;
        }

        if new {
            self.script.clear();
            self.script_name = "untitled.lua".to_owned();
            self.output.clear();
        }
        if let Some((range, label)) = accept {
            self.apply_completion(ctx, editor_id, range, &label);
        }
        if run {
            self.output.clear();
            Self::run(self.state, &self.script, &mut self.output);
        }
        if save {
            if let Some(dir) = &scripts_dir {
                self.save_script(dir);
            }
        }
        if delete {
            self.delete_script(scripts_dir.as_deref());
        }
        if let Some(path) = load {
            self.load_script(&path);
        }
    }

    /// Replaces the token at `range` with `label` and moves the caret to the end
    /// of the inserted text.
    fn apply_completion(&mut self, ctx: &Context, editor_id: Id, range: Range<usize>, label: &str) {
        let mut chars: Vec<char> = self.script.chars().collect();
        let end = range.end.min(chars.len());
        let start = range.start.min(end);
        let caret = start + label.chars().count();
        chars.splice(start..end, label.chars());
        self.script = chars.into_iter().collect();

        if let Some(mut state) = TextEditState::load(ctx, editor_id) {
            state
                .cursor
                .set_char_range(Some(CCursorRange::one(CCursor::new(caret))));
            state.store(ctx, editor_id);
        }
    }

    fn delete_script(&mut self, scripts_dir: Option<&Path>) {
        let Some(dir) = scripts_dir else { return };
        let mut name = self.script_name.trim().to_owned();
        if !name.ends_with(".lua") {
            name.push_str(".lua");
        }
        let path = dir.join(&name);
        if !path.is_file() {
            self.output.push_str(&format!("\n[no such script: {name}]\n"));
            return;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => self.output.push_str(&format!("\n[deleted scripts/{name}]\n")),
            Err(e) => self.output.push_str(&format!("\nfailed to delete {name}: {e}\n")),
        }
    }

    fn save_script(&mut self, dir: &Path) {
        let mut name = self.script_name.trim().to_owned();
        if !name.ends_with(".lua") {
            name.push_str(".lua");
        }
        let result = std::fs::create_dir_all(dir)
            .and_then(|_| std::fs::write(dir.join(&name), &self.script));
        match result {
            Ok(()) => self.output.push_str(&format!("\n[saved scripts/{name}]\n")),
            Err(e) => self.output.push_str(&format!("\nfailed to save {name}: {e}\n")),
        }
    }

    fn load_script(&mut self, path: &Path) {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                self.script = text;
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    self.script_name = name.to_owned();
                }
            }
            Err(e) => self.output.push_str(&format!("\nfailed to load: {e}\n")),
        }
    }

    /// Runs `script`, appending captured output/errors to `output` and merging
    /// an `EXPORT`ed project into the class list.
    fn run(state: StateRef, script: &str, output: &mut String) {
        let engine = match ScriptEngine::new() {
            Ok(e) => e,
            Err(e) => {
                output.push_str(&format!("failed to start Lua: {e}\n"));
                return;
            }
        };

        let pid = state.borrow().process.read().as_ref().map(|p| p.id());
        let host = GuiClassHost::new(state);
        let (printed, result) = engine.run_console_with_host(script, pid, host.clone());
        output.push_str(&printed);

        match result {
            Ok(Some(ron)) => Self::import(state, &ron, &host.deferred.borrow(), output),
            Ok(None) => {}
            Err(e) => output.push_str(&format!("\nerror: {e}\n")),
        }
    }

    /// Merges a project RON string into the GUI's class list (undoably via the
    /// normal undo stack): a class whose name already exists is updated in place,
    /// a new name is added, and classes the script didn't mention are kept.
    ///
    /// The merge happens at the schema level — the current classes are serialized
    /// back to a project, the incoming classes replace/extend them by name, and
    /// the union is reloaded — so cross-class pointer fields re-resolve by name.
    /// Per-class addresses and the current selection are carried over by name
    /// (they aren't part of the schema). `deferred` holds addresses the script
    /// set for classes that didn't exist yet; they're applied once the merge has
    /// created those classes.
    fn import(state: StateRef, ron: &str, deferred: &[(String, usize)], output: &mut String) {
        let Some(incoming) = ProjectData::from_str(ron) else {
            output.push_str("\n[EXPORT was not valid project RON]\n");
            return;
        };

        let state = &mut *state.borrow_mut();
        state.push_undo();

        // Remember runtime-only state (not captured by the schema) so we can
        // reattach it to same-named classes after the reload.
        let addresses: HashMap<String, usize> = state
            .class_list
            .classes()
            .iter()
            .map(|c| (c.name.clone(), c.address.get()))
            .collect();
        let selected_name = state.class_list.selected_class().map(|c| c.name.clone());

        // Update-or-add each incoming class into the current project by name.
        let mut merged = ProjectData::store(state.class_list.classes()).into_project();
        let (mut updated, mut added) = (0, 0);
        for class in incoming.into_project().classes {
            match merged.classes.iter_mut().find(|c| c.name == class.name) {
                Some(existing) => {
                    *existing = class;
                    updated += 1;
                }
                None => {
                    merged.classes.push(class);
                    added += 1;
                }
            }
        }

        let mut list = ProjectData::from_project(merged).load();
        for class in list.classes_mut() {
            if let Some(&addr) = addresses.get(&class.name) {
                class.address.set(addr);
            }
        }
        if let Some(id) = selected_name.and_then(|n| list.by_name(&n)).map(|c| c.id()) {
            *list.selected_mut() = Some(id);
        }

        // Apply addresses the script set for classes that only now exist. These
        // run after the carry-over above, so a script's address wins by design.
        let mut applied = 0;
        for (name, addr) in deferred {
            if let Some(class) = list.by_name(name) {
                class.address.set(*addr);
                applied += 1;
            }
        }

        state.class_list = list;
        state.dummy = false;
        output.push_str(&format!(
            "\n[merged classes into the GUI: {updated} updated, {added} added, \
             {applied} address(es) applied (Ctrl+Z to undo)]\n"
        ));
    }
}

/// The primary caret's char index within the editor, if any.
fn caret_index(output: &TextEditOutput) -> Option<usize> {
    output.cursor_range.map(|range| range.primary.index)
}

/// Renders the completion suggestions as a popup list under the editor. Returns
/// `Some((token_range, label))` when a suggestion is clicked.
fn render_completions(
    ui: &mut eframe::egui::Ui,
    comp: &Completion,
) -> Option<(Range<usize>, String)> {
    let mut chosen = None;
    Frame::popup(ui.style()).show(ui, |ui| {
        ScrollArea::vertical()
            .max_height(160.0)
            .show(ui, |ui| {
                for cand in comp.candidates.iter().take(12) {
                    let label = format!("{:<18} {}", cand.label, cand.detail);
                    if ui
                        .add(Button::new(label).frame(false))
                        .clicked()
                    {
                        chosen = Some((comp.range.clone(), cand.label.clone()));
                    }
                }
            });
    });
    chosen
}

/// Lists `*.lua` files in `dir`, sorted.
fn list_lua(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "lua"))
        .collect();
    v.sort();
    v
}
