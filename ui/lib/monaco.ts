// Self-host Monaco (no CDN) so it works inside the packaged Tauri app.
// Lua / Rust / C++ highlighting comes from Monaco's built-in Monarch grammars
// (no language worker needed); only the core editor worker is required.
import { loader } from "@monaco-editor/react";
import * as monaco from "monaco-editor";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";

(self as unknown as { MonacoEnvironment: monaco.Environment }).MonacoEnvironment = {
  getWorker() {
    return new EditorWorker();
  },
};

loader.config({ monaco });

/** Our shared dark editor theme, registered once. */
let themed = false;
export function ensureMonacoTheme() {
  if (themed) return;
  themed = true;
  monaco.editor.defineTheme("nemclass-dark", {
    base: "vs-dark",
    inherit: true,
    rules: [],
    colors: {
      "editor.background": "#12161f",
      "editor.foreground": "#e6edf3",
      "editorLineNumber.foreground": "#5a6473",
      "editorGutter.background": "#12161f",
      "editor.lineHighlightBackground": "#171c28",
      "editorCursor.foreground": "#4c8dff",
      "editor.selectionBackground": "#24405f",
    },
  });
}

export { monaco };
