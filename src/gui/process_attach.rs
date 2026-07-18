use crate::{gui::floating_window, state::StateRef};
use eframe::{
    egui::{Context, RichText, ScrollArea, TextEdit},
    epaint::FontId,
};
use nemclass_memory::external::{ProcessEntry, ProcessIterator};

pub struct ProcessAttachWindow {
    shown: bool,
    filter: String,
    processes: Vec<ProcessEntry>,
}

impl ProcessAttachWindow {
    pub fn new(_: StateRef) -> Self {
        Self {
            processes: vec![],
            shown: false,
            filter: "".to_owned(),
        }
    }

    pub fn toggle(&mut self) {
        self.shown = !self.shown;

        if self.shown {
            self.processes = collect_processes();
        }
    }

    pub fn show(&mut self, ctx: &Context) -> Option<u32> {
        let shown = self.shown;
        let mut attach_pid = None;
        let response = floating_window(
            ctx,
            shown,
            "nem_process_attach_viewport",
            "Attach to process",
            [220.0, 360.0],
            |ui| {
                ui.vertical_centered_justified(|ui| {
                    let r = TextEdit::singleline(&mut self.filter)
                        .desired_width(f32::INFINITY)
                        .hint_text("Filter by name")
                        .show(ui)
                        .response;

                    if ui.button("Refresh").clicked() || r.changed() {
                        self.processes = collect_processes();
                    }

                    ui.add_space(4.);
                    ui.separator();
                    ui.add_space(4.);

                    ScrollArea::vertical().show(ui, |ui| {
                        for pe in self.processes.iter().filter(|pe| {
                            self.filter.is_empty()
                                || pe.name.to_lowercase().contains(&self.filter.to_lowercase())
                        }) {
                            if ui
                                .button(
                                    RichText::new(format!("{} - {}", pe.name, pe.id))
                                        .font(FontId::proportional(16.)),
                                )
                                .clicked()
                            {
                                attach_pid = Some(pe.id);
                            }
                        }
                    });
                });
            },
        );

        if let Some((true, ())) = response {
            self.shown = false;
        }

        attach_pid
    }
}

fn collect_processes() -> Vec<ProcessEntry> {
    ProcessIterator::new().into_iter().flatten().collect()
}
