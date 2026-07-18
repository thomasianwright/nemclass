//! A Lua scripting console window. Runs scripts through the headless
//! [`ScriptEngine`], captures `print` output, exposes the attached process id as
//! the `PID` global, and — if a script sets the `EXPORT` global to a project RON
//! string — imports the declared classes into the GUI (undoably).

use crate::{project::ProjectData, state::StateRef};
use eframe::{
    egui::{Context, TextEdit, Window},
    epaint::FontId,
};
use nemclass_scripting::ScriptEngine;

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
    output: String,
}

impl ScriptConsole {
    pub fn new(state: StateRef) -> Self {
        Self {
            state,
            shown: false,
            script: DEFAULT_SCRIPT.to_owned(),
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

        Window::new("Lua console")
            .open(&mut self.shown)
            .default_size([560., 440.])
            .show(ctx, |ui| {
                if ui.button("Run").clicked() {
                    self.output.clear();
                    Self::run(self.state, &self.script, &mut self.output);
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
        let (printed, result) = engine.run_console(script, pid);
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
