// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // On Linux/Wayland, GTK draws client-side window decorations and loads their
    // icons (16px "status" icons) through the system SVG pixbuf loader. On
    // distros where that loader is glycin's sandboxed `glycin-svg`, it can abort
    // at startup (`Failed to load .../image-missing.svg`, SIGABRT). Running under
    // XWayland avoids it (the X window manager draws decorations, so GTK loads no
    // icon). Force the X11 GDK backend when running under Wayland unless the user
    // has explicitly chosen a backend.
    #[cfg(target_os = "linux")]
    {
        let has_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        let backend_set = std::env::var_os("GDK_BACKEND").is_some();
        let has_x11 = std::env::var_os("DISPLAY").is_some();
        if has_wayland && has_x11 && !backend_set {
            // SAFETY: set before any threads are spawned or GTK is initialised.
            std::env::set_var("GDK_BACKEND", "x11");
            // Under XWayland in nested / VM sessions, WebKit's DMABUF renderer
            // often can't allocate GBM buffers ("Failed to create GBM buffer"),
            // leaving a blank webview. Fall back to the non-DMABUF renderer.
            if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
                std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            }
        }
    }

    nemclass_tauri_lib::run()
}
