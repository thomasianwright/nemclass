#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(all(not(unix), not(windows)))]
compile_error!("Only UNIX and Windows platforms are supported.");

#[cfg(not(target_pointer_width = "64"))]
compile_error!("Only X64 targets are supported.");

mod address;
mod app;
mod class;
mod clipboard;
mod config;
mod context;
mod field;
mod generator;
mod gui;
mod hotkeys;
mod process;
mod project;
mod state;
mod value;

use config::YClassConfig;
use eframe::{
    egui::{FontData, FontDefinitions, IconData, Key, Modifiers, ViewportBuilder, Visuals},
    epaint::{FontFamily, FontId},
    NativeOptions,
};
use hotkeys::HotkeyManager;
use state::GlobalState;
use std::{cell::RefCell, sync::Arc};

/// Monospaced font id.
const FID_M: FontId = FontId::monospace(16.);

#[cfg(test)]
mod tests {
    #[test]
    fn window_icon_decodes() {
        let icon = super::load_icon().expect("assets/nemmem.ico should decode");
        assert!(icon.width > 0 && icon.height > 0);
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
    }
}

/// Decodes the embedded window icon (`assets/nemmem.ico`), or `None` if it can't
/// be decoded.
fn load_icon() -> Option<IconData> {
    let image = image::load_from_memory(include_bytes!("../assets/nemmem.ico"))
        .ok()?
        .to_rgba8();
    let (width, height) = image.dimensions();
    Some(IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

fn main() {
    let mut viewport = ViewportBuilder::default();
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }

    eframe::run_native(
        "YClass",
        NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(|cc| {
            let config = YClassConfig::load_or_default();
            cc.egui_ctx.set_visuals(Visuals::dark());
            cc.egui_ctx.set_pixels_per_point(config.dpi.unwrap_or(1.));

            let mut fonts = FontDefinitions::default();
            fonts.font_data.insert(
                "roboto-mono".into(),
                Arc::new(FontData::from_static(include_bytes!(
                    "../fonts/RobotoMono-Regular.ttf"
                ))),
            );
            fonts
                .families
                .get_mut(&FontFamily::Monospace)
                .unwrap()
                .insert(0, "roboto-mono".into());
            cc.egui_ctx.set_fonts(fonts);

            let mut hotkeys = HotkeyManager::default();
            hotkeys.register("attach_process", Key::A, Modifiers::ALT);
            hotkeys.register("attach_recent", Key::A, Modifiers::ALT | Modifiers::CTRL);
            hotkeys.register("detach_process", Key::D, Modifiers::ALT);

            Ok(Box::new(app::YClassApp::new(Box::leak(Box::new(
                RefCell::new(GlobalState {
                    config,
                    hotkeys,
                    ..Default::default()
                }),
            )))))
        }),
    )
    .unwrap();
}
