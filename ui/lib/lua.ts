// Monaco language support for the `nem` Lua scripting API: context-aware
// completions (functions, methods, kinds, globals + Lua keywords/stdlib) driven
// by the bundled LuaCATS definitions, plus a lightweight document formatter.
import { monaco } from "./monaco";

type ParamDoc = { name: string; type: string; desc: string };
type Fn = {
  recv: string;
  sep: "." | ":";
  name: string;
  params: string[];
  ret?: string;
  doc: string;
};
type Field = { cls: string; name: string; type: string; desc: string };
type Global = { name: string; type?: string; doc: string };
type Parsed = { fns: Fn[]; fields: Field[]; globals: Global[] };

/** Render a LuaCATS block into a Markdown documentation string. */
function docText(doc: string[], params: ParamDoc[], ret?: string): string {
  const parts: string[] = [];
  const body = doc.join("\n").trim();
  if (body) parts.push(body);
  if (params.length) {
    parts.push(
      params
        .map((p) => `- \`${p.name}\`: ${p.type}${p.desc ? ` — ${p.desc}` : ""}`)
        .join("\n"),
    );
  }
  if (ret) parts.push(`*returns* \`${ret}\``);
  return parts.join("\n\n");
}

/** Parse the `nem.lua` LuaCATS definitions into structured completion data. */
function parseDefs(defs: string): Parsed {
  const fns: Fn[] = [];
  const fields: Field[] = [];
  const globals: Global[] = [];

  let doc: string[] = [];
  let params: ParamDoc[] = [];
  let ret: string | undefined;
  let type: string | undefined;
  let cls: string | null = null;
  const reset = () => {
    doc = [];
    params = [];
    ret = undefined;
    type = undefined;
  };

  for (const raw of defs.split("\n")) {
    const line = raw.trim();
    let m: RegExpMatchArray | null;

    if ((m = line.match(/^---@class\s+([\w.]+)/))) {
      cls = m[1];
      reset();
      continue;
    }
    if ((m = line.match(/^---@field\s+(\w+)\s+(\S+)\s*(.*)$/))) {
      if (cls) fields.push({ cls, name: m[1], type: m[2], desc: m[3].trim() });
      continue;
    }
    if ((m = line.match(/^---@param\s+(\w+)\s+(\S+)\s*(.*)$/))) {
      params.push({ name: m[1], type: m[2], desc: m[3].trim() });
      continue;
    }
    if ((m = line.match(/^---@return\s+(.+)$/))) {
      ret = m[1].trim();
      continue;
    }
    if ((m = line.match(/^---@type\s+(.+)$/))) {
      type = m[1].trim();
      continue;
    }
    if (line.startsWith("---@")) continue; // @meta and other annotations
    if ((m = line.match(/^---\s?(.*)$/))) {
      if (m[1] || doc.length) doc.push(m[1]);
      continue;
    }

    if ((m = line.match(/^function\s+(\w+)([.:])(\w+)\s*\(([^)]*)\)/))) {
      const p = m[4].split(",").map((s) => s.trim()).filter(Boolean);
      fns.push({
        recv: m[1],
        sep: m[2] as "." | ":",
        name: m[3],
        params: p,
        ret,
        doc: docText(doc, params, ret),
      });
      reset();
      continue;
    }
    // Top-level globals: all-caps (PID/PNAME/PROJECT/EXPORT) or `nem`.
    if ((m = line.match(/^([A-Z_][A-Z0-9_]*|nem)\s*=/))) {
      globals.push({ name: m[1], type, doc: docText(doc, [], undefined) });
      cls = null;
      reset();
      continue;
    }
    if (line === "") reset();
  }
  return { fns, fields, globals };
}

const K = monaco.languages.CompletionItemKind;
const SNIPPET = monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet;
type Range = monaco.IRange;
type Item = monaco.languages.CompletionItem;

const md = (value: string) => (value ? { value } : undefined);

function fnItem(f: Fn, kind: monaco.languages.CompletionItemKind, range: Range): Item {
  const insertText = f.params.length
    ? `${f.name}(${f.params.map((p, i) => `\${${i + 1}:${p}}`).join(", ")})`
    : `${f.name}()`;
  return {
    label: f.name,
    kind,
    insertText,
    insertTextRules: SNIPPET,
    detail: `${f.recv}${f.sep}${f.name}(${f.params.join(", ")})${f.ret ? `: ${f.ret}` : ""}`,
    documentation: md(f.doc),
    range,
  };
}

function fieldItem(fl: Field, range: Range): Item {
  return {
    label: fl.name,
    kind: K.Field,
    insertText: fl.name,
    detail: `${fl.cls}.${fl.name}: ${fl.type}`,
    documentation: md(fl.desc),
    range,
  };
}

function globalItem(g: Global, range: Range): Item {
  return {
    label: g.name,
    kind: g.name === "nem" ? K.Module : K.Variable,
    insertText: g.name,
    detail: g.type ? `${g.name}: ${g.type}` : g.name,
    documentation: md(g.doc),
    range,
  };
}

const LUA_KEYWORDS = [
  "and", "break", "do", "else", "elseif", "end", "false", "for", "function",
  "if", "in", "local", "nil", "not", "or", "repeat", "return", "then", "true",
  "until", "while",
];

// Common Lua stdlib calls worth completing as snippets.
const LUA_STD: { label: string; insertText: string; detail: string }[] = [
  { label: "print", insertText: "print(${1})", detail: "print(...)" },
  { label: "pairs", insertText: "pairs(${1:t})", detail: "pairs(t)" },
  { label: "ipairs", insertText: "ipairs(${1:t})", detail: "ipairs(t)" },
  { label: "tostring", insertText: "tostring(${1})", detail: "tostring(v)" },
  { label: "tonumber", insertText: "tonumber(${1})", detail: "tonumber(v)" },
  { label: "type", insertText: "type(${1})", detail: "type(v)" },
  { label: "assert", insertText: "assert(${1})", detail: "assert(v, msg?)" },
  { label: "error", insertText: "error(${1})", detail: "error(msg)" },
  { label: "pcall", insertText: "pcall(${1:fn})", detail: "pcall(fn, ...)" },
  { label: "select", insertText: "select(${1:'#'}, ${2:...})", detail: "select(n, ...)" },
  { label: "string.format", insertText: 'string.format("${1}", ${2})', detail: "string.format(fmt, ...)" },
  { label: "string.rep", insertText: "string.rep(${1:s}, ${2:n})", detail: "string.rep(s, n)" },
  { label: "table.insert", insertText: "table.insert(${1:t}, ${2:v})", detail: "table.insert(t, v)" },
  { label: "table.concat", insertText: 'table.concat(${1:t}, ${2:", "})', detail: "table.concat(t, sep?)" },
  { label: "math.floor", insertText: "math.floor(${1})", detail: "math.floor(x)" },
];

// Control-flow scaffolding snippets.
const LUA_SNIPPETS: { label: string; insertText: string; detail: string }[] = [
  { label: "local", insertText: "local ${1:name} = ${2:value}", detail: "local binding" },
  { label: "function", insertText: "function ${1:name}(${2})\n\t$0\nend", detail: "function block" },
  { label: "if", insertText: "if ${1:cond} then\n\t$0\nend", detail: "if block" },
  { label: "ifelse", insertText: "if ${1:cond} then\n\t$2\nelse\n\t$0\nend", detail: "if/else block" },
  { label: "for", insertText: "for ${1:i} = ${2:1}, ${3:n} do\n\t$0\nend", detail: "numeric for" },
  { label: "forin", insertText: "for ${1:k}, ${2:v} in pairs(${3:t}) do\n\t$0\nend", detail: "generic for" },
  { label: "while", insertText: "while ${1:cond} do\n\t$0\nend", detail: "while block" },
];

function snippetItem(
  s: { label: string; insertText: string; detail: string },
  kind: monaco.languages.CompletionItemKind,
  range: Range,
): Item {
  return { label: s.label, kind, insertText: s.insertText, insertTextRules: SNIPPET, detail: s.detail, range };
}

// ---- Formatter -----------------------------------------------------------

/** Blank out string/comment content so keywords/brackets inside them don't
 *  affect indentation (leading whitespace is all that ever changes). */
function stripLua(s: string): string {
  let r = s
    .replace(/"(\\.|[^"\\])*"/g, '""')
    .replace(/'(\\.|[^'\\])*'/g, "''")
    .replace(/\[\[.*?\]\]/g, "[[]]");
  const c = r.indexOf("--");
  if (c >= 0) r = r.slice(0, c);
  return r;
}

/** Re-indent Lua source with two-space blocks. Heuristic but content-safe:
 *  only leading whitespace is rewritten, and long-bracket bodies are left
 *  verbatim. */
export function formatLua(src: string): string {
  const INDENT = "  ";
  const lines = src.replace(/\r\n?/g, "\n").split("\n");
  let level = 0;
  let inLong = false;
  const out: string[] = [];
  for (const raw of lines) {
    if (inLong) {
      out.push(raw);
      if (/\]\]/.test(raw)) inLong = false;
      continue;
    }
    const line = raw.trim();
    if (line === "") {
      out.push("");
      continue;
    }
    const code = stripLua(line);
    const thisLevel = /^(end\b|until\b|else\b|elseif\b|\}|\))/.test(code)
      ? Math.max(0, level - 1)
      : level;
    out.push(INDENT.repeat(thisLevel) + line);

    const opens =
      (code.match(/\b(function|then|do|repeat|else)\b/g) || []).length +
      (code.match(/[{(]/g) || []).length;
    const closes =
      (code.match(/\b(end|until|elseif|else)\b/g) || []).length +
      (code.match(/[})]/g) || []).length;
    level = Math.max(0, level + opens - closes);

    if (/--\[\[/.test(line) && !/\]\]/.test(line)) inLong = true;
    else if (/(^|[^-])\[\[/.test(code) && !/\]\]/.test(code)) inLong = true;
  }
  return out.join("\n");
}

// ---- Registration --------------------------------------------------------

let registered = false;

export function registerLuaCompletions(defs: string) {
  if (registered) return;
  registered = true;

  const { fns, fields, globals } = parseDefs(defs);
  const nemFns = fns.filter((f) => f.recv === "nem" && f.sep === ".");
  const kindsFns = fns.filter((f) => f.recv === "Kinds" && f.sep === ".");
  const nemFields = fields.filter((f) => f.cls === "nem"); // e.g. `kinds`
  const kindFields = fields.filter((f) => f.cls === "nem.Kinds");
  // Methods across all classes, deduped by name (no type inference).
  const methods = [...new Map(fns.filter((f) => f.sep === ":").map((f) => [f.name, f])).values()];

  monaco.languages.registerCompletionItemProvider("lua", {
    triggerCharacters: [".", ":"],
    provideCompletionItems(model, position) {
      const line = model.getValueInRange({
        startLineNumber: position.lineNumber,
        startColumn: 1,
        endLineNumber: position.lineNumber,
        endColumn: position.column,
      });
      const word = model.getWordUntilPosition(position);
      const range: Range = {
        startLineNumber: position.lineNumber,
        endLineNumber: position.lineNumber,
        startColumn: word.startColumn,
        endColumn: word.endColumn,
      };

      if (/\bnem\s*\.\s*kinds\s*\.\s*\w*$/.test(line)) {
        return {
          suggestions: [
            ...kindsFns.map((f) => fnItem(f, K.Function, range)),
            ...kindFields.map((fl) => fieldItem(fl, range)),
          ],
        };
      }
      if (/\bnem\s*\.\s*\w*$/.test(line)) {
        return {
          suggestions: [
            ...nemFns.map((f) => fnItem(f, K.Function, range)),
            ...nemFields.map((fl) => fieldItem(fl, range)),
          ],
        };
      }
      if (/[\w)\]"']:\w*$/.test(line)) {
        return { suggestions: methods.map((f) => fnItem(f, K.Method, range)) };
      }
      return {
        suggestions: [
          ...globals.map((g) => globalItem(g, range)),
          ...LUA_STD.map((s) => snippetItem(s, K.Function, range)),
          ...LUA_SNIPPETS.map((s) => snippetItem(s, K.Snippet, range)),
          ...LUA_KEYWORDS.map((kw) => ({
            label: kw,
            kind: K.Keyword,
            insertText: kw,
            range,
          })),
        ],
      };
    },
  });

  monaco.languages.registerDocumentFormattingEditProvider("lua", {
    provideDocumentFormattingEdits(model) {
      return [{ range: model.getFullModelRange(), text: formatLua(model.getValue()) }];
    },
  });
}
