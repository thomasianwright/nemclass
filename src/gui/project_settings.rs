//! Project settings window — edits the `.nemproj` manifest (name and the
//! optional auto-attach configuration) and writes it back to the project folder.

use crate::state::StateRef;
use eframe::egui::{Context, TextEdit, Window};
use nemclass_sdk::{AutoAttach, Manifest};

pub struct ProjectSettings {
    state: StateRef,
    shown: bool,
    /// Whether the edit buffers have been synced from the current manifest.
    synced: bool,
    name: String,
    auto_attach: bool,
    process_name: String,
    module_name: String,
}

impl ProjectSettings {
    pub fn new(state: StateRef) -> Self {
        Self {
            state,
            shown: false,
            synced: false,
            name: String::new(),
            auto_attach: false,
            process_name: String::new(),
            module_name: String::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.shown = !self.shown;
        if self.shown {
            self.sync();
        }
    }

    /// Loads the edit buffers from the open project's manifest.
    fn sync(&mut self) {
        let s = self.state.borrow();
        self.name = s.manifest.name.clone();
        match &s.manifest.auto_attach {
            Some(a) => {
                self.auto_attach = true;
                self.process_name = a.process_name.clone();
                self.module_name = a.module_name.clone().unwrap_or_default();
            }
            None => {
                self.auto_attach = false;
                self.process_name.clear();
                self.module_name.clear();
            }
        }
        self.synced = true;
    }

    pub fn show(&mut self, ctx: &Context) {
        if !self.shown {
            return;
        }
        if !self.synced {
            self.sync();
        }

        let has_project = self.state.borrow().last_opened_project.is_some();

        let mut shown = self.shown;
        let mut save = false;
        let mut attach_now = false;

        Window::new("Project settings")
            .open(&mut shown)
            .resizable(false)
            .show(ctx, |ui| {
                if !has_project {
                    ui.label("No project is open.");
                    return;
                }

                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.add(TextEdit::singleline(&mut self.name).desired_width(240.0));
                });

                ui.separator();
                ui.checkbox(&mut self.auto_attach, "Auto-attach when the project opens");
                ui.add_enabled_ui(self.auto_attach, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Process name: ");
                        ui.add(TextEdit::singleline(&mut self.process_name).desired_width(220.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Module filter:");
                        ui.add(TextEdit::singleline(&mut self.module_name).desired_width(220.0));
                    });
                    ui.weak(
                        "Module is optional — picks the process instance that has it \
                         loaded (e.g. the Wine game running that .dll/.exe).",
                    );
                });

                ui.separator();
                ui.horizontal(|ui| {
                    save = ui.button("Save").clicked();
                    attach_now = ui.button("Save & attach now").clicked();
                });
            });

        self.shown = shown;
        if !self.shown {
            self.synced = false;
        }

        if (save || attach_now) && self.persist() && attach_now {
            self.state.borrow_mut().attach_from_manifest();
        }
    }

    /// Validates the buffers, writes the manifest to disk, and updates the live
    /// state. Returns `false` (with a toast) if the config is invalid or the save
    /// failed.
    fn persist(&mut self) -> bool {
        let auto_attach = if self.auto_attach {
            let process_name = self.process_name.trim();
            if process_name.is_empty() {
                self.state
                    .borrow_mut()
                    .toasts
                    .error("Auto-attach needs a process name");
                return false;
            }
            let module = self.module_name.trim();
            Some(AutoAttach {
                process_name: process_name.to_owned(),
                module_name: (!module.is_empty()).then(|| module.to_owned()),
            })
        } else {
            None
        };

        let manifest = Manifest {
            name: self.name.trim().to_owned(),
            auto_attach,
        };

        let state = &mut *self.state.borrow_mut();
        let Some(dir) = state.last_opened_project.clone() else {
            state.toasts.error("No project is open");
            return false;
        };

        match nemclass_sdk::project::write_manifest(&dir, &manifest) {
            Ok(()) => {
                state.manifest = manifest;
                state.toasts.info("Saved project settings");
                true
            }
            Err(e) => {
                state.toasts.error(format!("Failed to save settings: {e}"));
                false
            }
        }
    }
}
