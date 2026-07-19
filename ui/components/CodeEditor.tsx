import Editor, { type OnMount } from "@monaco-editor/react";
import { ensureMonacoTheme } from "../lib/monaco";

export function CodeEditor({
  value,
  language,
  onChange,
  onMount,
  readOnly = false,
  path,
}: {
  value: string;
  language: string;
  onChange?: (v: string) => void;
  onMount?: OnMount;
  readOnly?: boolean;
  path?: string;
}) {
  return (
    <Editor
      theme="nemclass-dark"
      language={language}
      value={value}
      path={path}
      beforeMount={() => ensureMonacoTheme()}
      onMount={onMount}
      onChange={(v) => onChange?.(v ?? "")}
      loading={<div className="p-3 text-xs text-faint">Loading editor…</div>}
      options={{
        readOnly,
        fontSize: 12,
        fontFamily: "JetBrains Mono, ui-monospace, monospace",
        minimap: { enabled: false },
        scrollBeyondLastLine: false,
        automaticLayout: true,
        tabSize: 2,
        renderLineHighlight: readOnly ? "none" : "line",
        smoothScrolling: true,
        padding: { top: 8 },
        scrollbar: { verticalScrollbarSize: 10, horizontalScrollbarSize: 10 },
      }}
    />
  );
}
