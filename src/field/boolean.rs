use super::{
    display_field_name, display_field_prelude, display_field_value, field_row_response, next_id,
    CodegenData, Field, FieldId, FieldKind, FieldResponse, NamedState,
};
use crate::{context::InspectionContext, generator::Generator};
use eframe::{
    egui::{Label, Sense, Ui},
    epaint::{text::LayoutJob, Color32},
};
use std::slice;

pub struct BoolField {
    id: FieldId,
    state: NamedState,
}

impl BoolField {
    pub fn new(name: String) -> Self {
        Self {
            id: next_id(),
            state: NamedState::new(name),
        }
    }
}

impl Field for BoolField {
    fn id(&self) -> FieldId {
        self.id
    }

    fn name(&self) -> Option<String> {
        Some(self.state.name.borrow().clone())
    }

    fn size(&self) -> usize {
        1
    }

    fn kind(&self) -> FieldKind {
        FieldKind::Bool
    }

    fn clone_box(&self) -> Box<dyn Field> {
        Box::new(BoolField::new(self.state.name.borrow().clone()))
    }

    fn draw(&self, ui: &mut Ui, ctx: &mut InspectionContext) -> Option<FieldResponse> {
        let mut val = 0u8;
        let address = ctx.address + ctx.offset;
        let _ = ctx.process.read(address, slice::from_mut(&mut val));

        let mut resp = None;
        ui.horizontal(|ui| {
            let mut job = LayoutJob::default();
            display_field_prelude(ui.ctx(), self, ctx, &mut job);

            let r = ui.add(Label::new(job).sense(Sense::click()));
            resp = field_row_response(&r, self, ctx);

            display_field_name(self, ui, ctx, &self.state, Color32::GOLD);
            display_field_value(
                self,
                ui,
                ctx,
                &self.state,
                Color32::WHITE,
                |_| {
                    match val {
                        1 => "true",
                        0 => "false",
                        _ => "invalid",
                    }
                    .to_owned()
                },
                |new: &str| match new {
                    "1" | "true" | "yes" | "on" => {
                        ctx.process.write(address, &[1]);
                        true
                    }
                    "0" | "false" | "no" | "off" => {
                        ctx.process.write(address, &[0]);
                        true
                    }
                    _ => false,
                },
            );
        });

        ctx.offset += 1;
        resp
    }

    fn codegen(&self, generator: &mut dyn Generator, _: &CodegenData) {
        generator.add_field(self.state.name.borrow().as_str(), FieldKind::Bool, None);
    }
}
