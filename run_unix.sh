#!/bin/sh
# Build and run the NemClass (Tauri) UI on Linux.
#
# IMPORTANT: do NOT run the GUI as root. Running the webview as root crashes the
# system SVG icon loader (glycin's sandbox) at startup. Instead we grant the
# binary CAP_SYS_PTRACE so it can attach to / read other processes' memory while
# running as your normal user.
set -e

# 1. Build the production app via the Tauri CLI. This runs the frontend build
#    (beforeBuildCommand) AND generates the *production* context that embeds the
#    assets — plain `cargo build --release` leaves the binary in dev mode
#    (it tries to connect to the dev server and shows a blank page).
pnpm install
pnpm exec tauri build --no-bundle
BIN="./target/release/nemclass_tauri"

# 3. Allow attaching without running as root. Either grant the binary the
#    capability (preferred, scoped to this binary)...
sudo setcap cap_sys_ptrace+ep "$BIN"
#    ...or, alternatively, lower the system-wide restriction for this session:
#    echo 0 | sudo tee /proc/sys/kernel/yama/ptrace_scope

# 4. Run as the normal user. Force the X11 backend if a nested Wayland
#    compositor rejects the webview ("Gdk-Message: Error 71").
GDK_BACKEND=x11 WEBKIT_DISABLE_DMABUF_RENDERER=1 exec "$BIN"
