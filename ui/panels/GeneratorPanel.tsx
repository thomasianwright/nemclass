import { Copy, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { CodeEditor } from "../components/CodeEditor";
import { api } from "../lib/api";
import { useStore } from "../store";

export function GeneratorPanel() {
  const classRev = useStore((s) => s.classRev);
  const guard = useStore((s) => s.guard);
  const toast = useStore((s) => s.toast);
  const [langs, setLangs] = useState<string[]>(["Rust", "C++"]);
  const [lang, setLang] = useState("Rust");
  const [code, setCode] = useState("");

  useEffect(() => {
    api.genLangs().then((l) => {
      if (l.length) {
        setLangs(l);
        setLang((cur) => (l.includes(cur) ? cur : l[0]));
      }
    });
  }, []);

  const gen = async () => {
    const c = await guard(() => api.generateCode(lang));
    if (c !== undefined) setCode(c);
  };

  useEffect(() => {
    gen();
  }, [lang, classRev]);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      toast("Copied to clipboard", "success");
    } catch {
      toast("Clipboard unavailable", "error");
    }
  };

  const monacoLang = lang.toLowerCase().includes("rust") ? "rust" : "cpp";

  return (
    <div className="panel-body flex flex-col">
      <div className="flex items-center gap-2 border-b border-border bg-panel-2 px-2 py-1.5">
        <select
          className="input mono w-28"
          value={lang}
          onChange={(e) => setLang(e.target.value)}
        >
          {langs.map((l) => (
            <option key={l} value={l}>
              {l}
            </option>
          ))}
        </select>
        <button className="btn" onClick={gen} title="Regenerate">
          <RefreshCw size={13} /> Generate
        </button>
        <div className="flex-1" />
        <button className="btn" onClick={copy} title="Copy to clipboard">
          <Copy size={13} /> Copy
        </button>
      </div>
      <div className="min-h-0 flex-1">
        <CodeEditor value={code} language={monacoLang} readOnly />
      </div>
    </div>
  );
}
