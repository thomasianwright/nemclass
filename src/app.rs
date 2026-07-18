use crate::{
    class::ClassId,
    context::Selection,
    field::{allocate_padding, FieldId, FieldKind, FieldKindExt},
    gui::{ClassListPanel, InspectorPanel, ToolBarPanel, ToolBarResponse},
    process::Process,
    state::{GlobalState, StateRef},
};
use eframe::{
    egui::{Key, Ui, ViewportCommand},
    epaint::Color32,
    App, Frame,
};
use std::{sync::Once, time::Duration};

pub struct YClassApp {
    class_list: ClassListPanel,
    inspector: InspectorPanel,
    tool_bar: ToolBarPanel,
    state: StateRef,
}

impl YClassApp {
    pub fn new(state: StateRef) -> Self {
        Self {
            class_list: ClassListPanel::new(state),
            inspector: InspectorPanel::new(state),
            tool_bar: ToolBarPanel::new(state),
            state,
        }
    }
}

impl App for YClassApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        ctx.request_repaint_after(Duration::from_millis(100));

        static DPI_INIT: Once = Once::new();
        DPI_INIT.call_once(|| {
            let dpi = self.state.borrow().config.dpi.unwrap_or(1.);
            ctx.set_pixels_per_point(dpi);
        });

        // Undo (Ctrl+Z) / Redo (Ctrl+Shift+Z), unless a text field is capturing the keystroke.
        if !ctx.egui_wants_keyboard_input() {
            let (undo, redo) = ctx.input(|i| {
                let z = i.key_pressed(Key::Z) && i.modifiers.command;
                (z && !i.modifiers.shift, z && i.modifiers.shift)
            });
            if undo {
                self.state.borrow_mut().undo();
            } else if redo {
                self.state.borrow_mut().redo();
            }
        }

        match self.tool_bar.show(ctx) {
            Some(ToolBarResponse::Add(n)) => {
                let state = &mut *self.state.borrow_mut();

                let cid = state
                    .selection
                    .map(|s| s.container_id)
                    .or_else(|| state.class_list.selected());
                if let Some(cid) = cid.filter(|cid| state.class_list.by_id(*cid).is_some()) {
                    state.push_undo();
                    if let Some(class) = state.class_list.by_id_mut(cid) {
                        class.fields.extend(allocate_padding(n));
                        state.dummy = false;
                    }
                }
            }
            Some(ToolBarResponse::Remove(n)) => {
                let state = &mut *self.state.borrow_mut();

                if let Some(Selection {
                    container_id,
                    field_id,
                    ..
                }) = state.selection
                {
                    let pos = state
                        .class_list
                        .by_id(container_id)
                        .and_then(|c| c.fields.iter().position(|f| f.id() == field_id));

                    if let Some(pos) = pos {
                        state.push_undo();
                        // Removal starts at the selected field, so the selection is consumed.
                        state.selection = None;
                        if let Some(class) = state.class_list.by_id_mut(container_id) {
                            let from = pos.min(class.fields.len());
                            let to = (pos + n).min(class.fields.len());
                            class.fields.drain(from..to);
                            state.dummy = false;
                        }
                    } else {
                        // Selection referenced a field/class that no longer exists.
                        state.selection = None;
                    }
                }
            }
            Some(ToolBarResponse::Insert(n)) => {
                let state = &mut *self.state.borrow_mut();

                if let Some(Selection {
                    container_id,
                    field_id,
                    ..
                }) = state.selection
                {
                    let pos = state
                        .class_list
                        .by_id(container_id)
                        .and_then(|c| c.fields.iter().position(|f| f.id() == field_id));

                    if let Some(pos) = pos {
                        state.push_undo();
                        if let Some(class) = state.class_list.by_id_mut(container_id) {
                            let mut padding = allocate_padding(n);
                            while let Some(field) = padding.pop() {
                                class.fields.insert(pos, field);
                            }
                            state.dummy = false;
                        }
                    } else {
                        state.selection = None;
                    }
                }
            }
            Some(ToolBarResponse::ChangeKind(new)) => {
                let state = &mut *self.state.borrow_mut();

                if let Some(Selection {
                    container_id,
                    field_id,
                    ..
                }) = state.selection
                {
                    change_field_kind(state, container_id, field_id, new);
                }
            }
            Some(ToolBarResponse::ProcessDetach) => {
                let mut state = self.state.borrow_mut();

                if let Some(mut process) = state
                    .process
                    .clone() /* ??? */
                    .try_write()
                {
                    *process = None;
                    ctx.send_viewport_cmd(ViewportCommand::Title("YClass".to_owned()));
                } else {
                    state.toasts.warning("Process is currently in use");
                }
            }
            Some(ToolBarResponse::ProcessAttach(pid)) => {
                let mut state = self.state.borrow_mut();

                if let Some(mut process) = state
                    .process
                    .clone() /* ??? */
                    .try_write()
                {
                    match Process::attach(pid, &state.config) {
                        Ok(proc) => {
                            ctx.send_viewport_cmd(ViewportCommand::Title(format!(
                                "YClass - Attached to {pid}"
                            )));
                            // Remember the native process name for quick re-attach
                            // (managed plugins don't expose a real name).
                            if !proc.is_managed() {
                                let name = proc.name();
                                if !name.is_empty() {
                                    state.config.last_attached_process_name = Some(name);
                                    state.config.save();
                                }
                            }

                            *process = Some(proc);
                        }
                        Err(e) => {
                            state.toasts.error(format!(
                                "Failed to attach to process.\nPossibly plugin error.\n{e}"
                            ));
                        }
                    }
                } else {
                    state.toasts.warning("Process is currently in use");
                }
            }
            None => {}
        }

        self.class_list.show(ctx);
        self.inspector.show(ctx);

        let mut style = (*ctx.global_style()).clone();
        let saved = style.clone();
        style.visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(0x10, 0x10, 0x10);
        style.visuals.widgets.noninteractive.fg_stroke.color = Color32::LIGHT_GRAY;
        ctx.set_global_style(style);

        self.state.borrow_mut().toasts.show(ctx);
        ctx.set_global_style(saved);
    }
}

pub fn is_valid_ident(name: &str) -> bool {
    !name.starts_with(char::is_numeric) && !name.contains(char::is_whitespace) && !name.is_empty()
}

/// Converts the field identified by `field_id` inside class `container_id` to `new`, preserving its
/// name. Shrinking pads the freed bytes; growing "steals" bytes from the following fields (padding
/// any remainder). Keeps the retyped field selected. Shared by the toolbar type buttons and the
/// inference "Guess type" / Hex-hint conversions.
pub fn change_field_kind(
    state: &mut GlobalState,
    container_id: ClassId,
    field_id: FieldId,
    new: FieldKind,
) {
    // Confirm the field exists before snapshotting, so no-op conversions don't pollute undo.
    let exists = state
        .class_list
        .by_id(container_id)
        .is_some_and(|c| c.fields.iter().any(|f| f.id() == field_id));
    if !exists {
        return;
    }
    state.push_undo();

    let Some(class) = state.class_list.by_id_mut(container_id) else {
        return;
    };
    let Some(pos) = class.fields.iter().position(|f| f.id() == field_id) else {
        return;
    };

    let (old_size, old_name) = (class.fields[pos].size(), class.fields[pos].name());
    let new_field_id;

    if old_size > new.size() {
        let mut padding = allocate_padding(old_size - new.size());
        class.fields[pos] = new.into_field(old_name);
        new_field_id = class.fields[pos].id();
        while let Some(pad) = padding.pop() {
            class.fields.insert(pos + 1, pad);
        }
    } else {
        let (mut steal_size, mut steal_len) = (0, 0);
        while steal_size < new.size() {
            let index = pos + steal_len;
            if index >= class.fields.len() {
                break;
            }
            steal_size += class.fields[index].size();
            steal_len += 1;
        }

        if steal_size < new.size() {
            state.toasts.error("Not enough space for a new field");
            return;
        }

        class.fields.drain(pos..pos + steal_len);
        let mut padding = allocate_padding(steal_size - new.size());
        class.fields.insert(pos, new.into_field(old_name));
        new_field_id = class.fields[pos].id();
        while let Some(pad) = padding.pop() {
            class.fields.insert(pos + 1, pad);
        }
    }

    if let Some(sel) = state.selection.as_mut() {
        if sel.field_id == field_id {
            sel.field_id = new_field_id;
        }
    }
    state.dummy = false;
}
