use super::{
    create_text_format, display_field_name, display_field_prelude, display_field_value,
    field_row_response, next_id, CodegenData, Field, FieldId, FieldKind, FieldResponse, FloatWidth,
    NamedState,
};
use crate::{context::InspectionContext, generator::Generator};
use eframe::{
    egui::{collapsing_header::CollapsingState, Grid, Label, Sense, Ui},
    epaint::{text::LayoutJob, Color32},
};

/// A row-major matrix of floating point components (e.g. a 4x4 view matrix), each of width
/// `f32`/`f64`. Rendered as a collapsible grid of individually editable cells.
pub struct MatrixField {
    id: FieldId,
    rows: u8,
    cols: u8,
    width: FloatWidth,
    state: NamedState,
}

impl MatrixField {
    pub fn new(rows: u8, cols: u8, width: FloatWidth, name: String) -> Self {
        Self {
            id: next_id(),
            rows,
            cols,
            width,
            state: NamedState::new(name),
        }
    }

    fn show_header(&self, ui: &mut Ui, ctx: &mut InspectionContext) -> Option<FieldResponse> {
        let mut job = LayoutJob::default();
        display_field_prelude(ui.ctx(), self, ctx, &mut job);

        let r = ui.add(Label::new(job).sense(Sense::click()));
        let menu_resp = field_row_response(&r, self, ctx);

        display_field_name(self, ui, ctx, &self.state, Color32::LIGHT_RED);

        let mut job = LayoutJob::default();
        job.append(
            &format!("[{}x{} {}]", self.rows, self.cols, self.width.rust_ty()),
            4.,
            create_text_format(ctx.is_selected(self.id), Color32::LIGHT_GRAY),
        );
        let r = ui.add(Label::new(job).sense(Sense::click()));
        if r.clicked() {
            ctx.select(self.id);
        }

        menu_resp
    }

    fn show_body(&self, ui: &mut Ui, ctx: &mut InspectionContext, base_offset: usize) {
        let size = self.size();
        let base_address = ctx.address + base_offset;
        let width = self.width;
        let wsz = width.size();

        let mut buf = vec![0u8; size];
        let _ = ctx.process.read(base_address, &mut buf);
        let process = ctx.process;

        let (rows, cols) = (self.rows as usize, self.cols as usize);
        Grid::new(ctx.current_id.with("_matrix_grid"))
            .striped(true)
            .show(ui, |ui| {
                for r in 0..rows {
                    for c in 0..cols {
                        let index = r * cols + c;
                        ctx.offset = base_offset + index * wsz;
                        let caddr = base_address + index * wsz;

                        display_field_value(
                            self,
                            ui,
                            ctx,
                            &self.state,
                            Color32::WHITE,
                            |_| width.read(&buf[index * wsz..]),
                            |new| {
                                if let Some(bytes) = width.parse_to_ne_bytes(new) {
                                    process.write(caddr, &bytes);
                                    true
                                } else {
                                    false
                                }
                            },
                        );
                    }
                    ui.end_row();
                }
            });

        ctx.offset = base_offset;
    }
}

impl Field for MatrixField {
    fn id(&self) -> FieldId {
        self.id
    }

    fn size(&self) -> usize {
        self.rows as usize * self.cols as usize * self.width.size()
    }

    fn name(&self) -> Option<String> {
        Some(self.state.name.borrow().clone())
    }

    fn kind(&self) -> FieldKind {
        FieldKind::Matrix {
            rows: self.rows,
            cols: self.cols,
            width: self.width,
        }
    }

    fn clone_box(&self) -> Box<dyn Field> {
        Box::new(MatrixField::new(
            self.rows,
            self.cols,
            self.width,
            self.state.name.borrow().clone(),
        ))
    }

    fn draw(&self, ui: &mut Ui, ctx: &mut InspectionContext) -> Option<FieldResponse> {
        let size = self.size();
        let base_offset = ctx.offset;

        let state = CollapsingState::load_with_default_open(ui.ctx(), ctx.current_id, false);
        let (_, header, _body) = state
            .show_header(ui, |ui| self.show_header(ui, ctx))
            .body(|ui| self.show_body(ui, ctx, base_offset));

        ctx.offset = base_offset + size;
        header.inner
    }

    fn codegen(&self, generator: &mut dyn Generator, _: &CodegenData) {
        generator.add_field(
            self.state.name.borrow().as_str(),
            FieldKind::Matrix {
                rows: self.rows,
                cols: self.cols,
                width: self.width,
            },
            None,
        );
    }
}
