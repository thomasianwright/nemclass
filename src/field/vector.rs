use super::{
    display_field_name, display_field_prelude, display_field_value, field_row_response, next_id,
    CodegenData, Field, FieldId, FieldKind, FieldResponse, FloatWidth, NamedState,
};
use crate::{context::InspectionContext, generator::Generator, process::Process};
use eframe::{
    egui::{Label, Sense, Ui},
    epaint::{text::LayoutJob, Color32},
};

/// A fixed-length vector of floating point components (`Vector2/3/4`), each of width `f32`/`f64`.
/// Components are laid out contiguously and each is individually editable.
pub struct VectorField {
    id: FieldId,
    components: u8,
    width: FloatWidth,
    state: NamedState,
}

impl VectorField {
    pub fn new(components: u8, width: FloatWidth, name: String) -> Self {
        Self {
            id: next_id(),
            components,
            width,
            state: NamedState::new(name),
        }
    }
}

fn write_component(process: &Process, address: usize, width: FloatWidth, new: &str) -> bool {
    if let Some(bytes) = width.parse_to_ne_bytes(new) {
        process.write(address, &bytes);
        true
    } else {
        false
    }
}

impl Field for VectorField {
    fn id(&self) -> FieldId {
        self.id
    }

    fn size(&self) -> usize {
        self.components as usize * self.width.size()
    }

    fn name(&self) -> Option<String> {
        Some(self.state.name.borrow().clone())
    }

    fn kind(&self) -> FieldKind {
        FieldKind::Vector {
            components: self.components,
            width: self.width,
        }
    }

    fn clone_box(&self) -> Box<dyn Field> {
        Box::new(VectorField::new(
            self.components,
            self.width,
            self.state.name.borrow().clone(),
        ))
    }

    fn draw(&self, ui: &mut Ui, ctx: &mut InspectionContext) -> Option<FieldResponse> {
        let size = self.size();
        let base_offset = ctx.offset;
        let base_address = ctx.address + base_offset;
        let width = self.width;
        let wsz = width.size();

        let mut buf = vec![0u8; size];
        let _ = ctx.process.read(base_address, &mut buf);
        // Copy the `&Process` reference out of `ctx` so per-component write closures don't
        // borrow `ctx` while it's also passed mutably to `display_field_value`.
        let process = ctx.process;

        let mut resp = None;
        ui.horizontal(|ui| {
            let mut job = LayoutJob::default();
            display_field_prelude(ui.ctx(), self, ctx, &mut job);

            let r = ui.add(Label::new(job).sense(Sense::click()));
            resp = field_row_response(&r, self, ctx);

            display_field_name(self, ui, ctx, &self.state, Color32::LIGHT_RED);

            for i in 0..self.components as usize {
                // Point `ctx.offset` at this component so the edit box keys on its address.
                ctx.offset = base_offset + i * wsz;
                let caddr = base_address + i * wsz;

                display_field_value(
                    self,
                    ui,
                    ctx,
                    &self.state,
                    Color32::WHITE,
                    |_| width.read(&buf[i * wsz..]),
                    |new| write_component(process, caddr, width, new),
                );
            }

            ctx.offset = base_offset;
        });

        ctx.offset = base_offset + size;
        resp
    }

    fn codegen(&self, generator: &mut dyn Generator, _: &CodegenData) {
        generator.add_field(
            self.state.name.borrow().as_str(),
            FieldKind::Vector {
                components: self.components,
                width: self.width,
            },
            None,
        );
    }
}
