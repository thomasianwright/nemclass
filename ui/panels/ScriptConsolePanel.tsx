import { FilePlus2, Play, Save } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { CodeEditor } from "../components/CodeEditor";
import { api } from "../lib/api";
import { registerLuaCompletions } from "../lib/lua";
import { useStore } from "../store";

const DEFAULT = `-- nem.* scripting API. Press Ctrl/Cmd+Enter to run.
print("classes:", table.concat(nem.classes(), ", "))
if PID then print("attached pid:", PID) end
`;

export function ScriptConsolePanel() {
  const project = useStore((s) => s.project);
  const mutated = useStore((s) => s.mutated);
  const toast = useStore((s) => s.toast);

  const [code, setCode] = useState(DEFAULT);
  const [output, setOutput] = useState("");
  const [scripts, setScripts] = useState<string[]>([]);
  const [name, setName] = useState("");

  useEffect(() => {
    api.scriptDefinitions().then(registerLuaCompletions).catch(() => {});
  }, []);

  const loadList = async () => setScripts(await api.scriptList().catch(() => []));
  useEffect(() => {
    loadList();
  }, [project?.dir]);

  const run = async () => {
    let r;
    try {
      r = await api.scriptRun(code);
    } catch (e) {
      setOutput((p) => p + `\n[error] ${String(e)}\n`);
      return;
    }
    let log = "> run\n";
    if (r.output) log += r.output;
    if (r.error) log += `[error] ${r.error}\n`;
    if (r.merged) {
      log += "[merged EXPORT into classes]\n";
      await mutated();
    }
    setOutput((p) => (p ? p + "\n" : "") + log);
  };

  const runRef = useRef(run);
  runRef.current = run;

  const onMount = (editor: any, m: any) => {
    editor.addCommand(m.KeyMod.CtrlCmd | m.KeyCode.Enter, () => runRef.current());
  };

  const load = async (n: string) => {
    if (!n) return;
    setName(n);
    const c = await api.scriptLoad(n).catch(() => "");
    if (c) setCode(c);
  };

  const save = async () => {
    if (!project?.dir) {
      toast("Open a project to save scripts", "error");
      return;
    }
    const n = name.trim();
    if (!n) {
      toast("Enter a script name", "error");
      return;
    }
    const ok = await api.scriptSave(n, code).then(() => true).catch((e) => {
      toast(String(e), "error");
      return false;
    });
    if (ok) {
      toast("Script saved", "success");
      await loadList();
    }
  };

  return (
    <div className="panel-body flex flex-col">
      <div className="flex items-center gap-1.5 border-b border-border bg-panel-2 px-2 py-1.5">
        <select
          className="input mono w-36"
          value=""
          onChange={(e) => load(e.target.value)}
          title="Load script"
        >
          <option value="">open…</option>
          {scripts.map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
        <input
          className="input mono w-40"
          placeholder="name.lua"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <button className="btn btn-ghost btn-icon" title="New" onClick={() => { setCode(DEFAULT); setName(""); }}>
          <FilePlus2 size={14} />
        </button>
        <button className="btn" title="Save" onClick={save}>
          <Save size={13} /> Save
        </button>
        <div className="flex-1" />
        <button className="btn btn-primary" onClick={run} title="Run (Ctrl+Enter)">
          <Play size={13} /> Run
        </button>
      </div>

      <div className="min-h-0 flex-1">
        <CodeEditor value={code} language="lua" onChange={setCode} onMount={onMount} />
      </div>

      <div className="flex h-40 flex-col border-t border-border">
        <div className="flex items-center justify-between bg-panel-2 px-2 py-0.5 text-[10px] uppercase tracking-wide text-faint">
          <span>output</span>
          <button className="hover:text-text" onClick={() => setOutput("")}>
            clear
          </button>
        </div>
        <pre className="mono flex-1 overflow-auto whitespace-pre-wrap px-2 py-1 text-[11px] text-text">
          {output || <span className="text-faint">Run a script to see output.</span>}
        </pre>
      </div>
    </div>
  );
}
