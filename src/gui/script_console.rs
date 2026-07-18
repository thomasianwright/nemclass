//! A Lua scripting console window. Runs scripts through the headless
//! [`ScriptEngine`], captures `print` output, exposes the attached process id as
//! the `PID` global, and — if a script sets the `EXPORT` global to a project RON
//! string — imports the declared classes into the GUI (undoably).
//!
//! Scripts are loaded from and saved to the open project's `scripts/` folder, so
//! they can be version-controlled alongside the project.

use crate::{project::ProjectData, state::StateRef};
use eframe::{
    egui::{Button, Context, TextEdit, Window},
    epaint::FontId,
};
use nemclass_scripting::{ClassHost, ScriptEngine};
use std::path::{Path, PathBuf};

/// Bridges scripts to the GUI's live class list, so `nem.set_class_address` and
/// friends can drive the inspector.
#[derive(Clone, Copy)]
struct GuiClassHost(StateRef);

impl ClassHost for GuiClassHost {
    fn class_names(&self) -> Vec<String> {
        self.0
            .try_borrow()
            .map(|s| s.class_list.classes().iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default()
    }

    fn set_class_address(&self, name: &str, address: usize) -> bool {
        if let Ok(s) = self.0.try_borrow() {
            if let Some(class) = s.class_list.by_name(name) {
                class.address.set(address);
                return true;
            }
        }
        false
    }

    fn class_address(&self, name: &str) -> Option<usize> {
        self.0
            .try_borrow()
            .ok()
            .and_then(|s| s.class_list.by_name(name).map(|c| c.address.get()))
    }
}

const DEFAULT_SCRIPT: &str = r#"-- nem.* scripting API. `PID` is the attached process id (or nil).
-- Set EXPORT to a project RON string to import classes into the GUI.

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

        // Collect actions inside the closure; apply them after (avoids borrow clashes).
        let mut run = false;
        let mut new = false;
        let mut save = false;
        let mut load: Option<PathBuf> = None;
        let mut shown = self.shown;

        Window::new("Lua console")
            .open(&mut shown)
            .default_size([600.0, 480.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    run = ui.button("Run").clicked();
                    new = ui.button("New").clicked();
                    ui.separator();
                    ui.label("File:");
                    ui.add(TextEdit::singleline(&mut self.script_name).desired_width(140.0));
                    let can_save =
                        scripts_dir.is_some() && !self.script_name.trim().is_empty();
                    save = ui.add_enabled(can_save, Button::new("Save")).clicked();
                });

                if !scripts.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Open:");
                        for path in &scripts {
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                if ui.button(name).clicked() {
                                    load = Some(path.clone());
                                }
                            }
                        }
                    });
                }

                ui.separator();
                TextEdit::multiline(&mut self.script)
                    .code_editor()
                    .desired_rows(16)
                    .desired_width(f32::INFINITY)
                    .font(FontId::monospace(12.))
                    .show(ui);

                ui.separator();
                ui.label("Output:");
                TextEdit::multiline(&mut self.output.as_str())
                    .desired_width(f32::INFINITY)
                    .font(FontId::monospace(12.))
                    .show(ui);
            });

        self.shown = shown;

        if new {
            self.script.clear();
            self.script_name = "untitled.lua".to_owned();
            self.output.clear();
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
        if let Some(path) = load {
            self.load_script(&path);
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

    /// Runs `script`, appending captured output/errors to `output` and importing
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
        let (printed, result) = engine.run_console_with_host(script, pid, GuiClassHost(state));
        output.push_str(&printed);

        match result {
            Ok(Some(ron)) => Self::import(state, &ron, output),
            Ok(None) => {}
            Err(e) => output.push_str(&format!("\nerror: {e}\n")),
        }
    }

    /// Imports a project RON string into the GUI's class list (replacing it,
    /// undoably via the normal undo stack).
    fn import(state: StateRef, ron: &str, output: &mut String) {
        match ProjectData::from_str(ron) {
            Some(pd) => {
                let state = &mut *state.borrow_mut();
                state.push_undo();
                state.class_list = pd.load();
                state.dummy = false;
                output.push_str("\n[imported classes into the GUI (Ctrl+Z to undo)]\n");
            }
            None => output.push_str("\n[EXPORT was not valid project RON]\n"),
        }
    }
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
