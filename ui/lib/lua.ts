// Registers a Monaco completion provider for the `nem` Lua API, driven by the
// bundled LuaCATS definitions (parsed for `nem.*` functions and `:` methods).
import { monaco } from "./monaco";

let registered = false;

export function registerLuaCompletions(defs: string) {
  if (registered) return;
  registered = true;

  const nemMembers = [
    ...new Set([...defs.matchAll(/function nem\.(\w+)/g)].map((m) => m[1])),
  ];
  const methods = [
    ...new Set([...defs.matchAll(/function \w+:(\w+)/g)].map((m) => m[1])),
  ];

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
      const range = {
        startLineNumber: position.lineNumber,
        endLineNumber: position.lineNumber,
        startColumn: word.startColumn,
        endColumn: word.endColumn,
      };
      const mk = (labels: string[], kind: monaco.languages.CompletionItemKind) =>
        labels.map((label) => ({ label, kind, insertText: label, range }));

      if (/nem\.\w*$/.test(line)) {
        return { suggestions: mk(nemMembers, monaco.languages.CompletionItemKind.Function) };
      }
      if (/:\w*$/.test(line)) {
        return { suggestions: mk(methods, monaco.languages.CompletionItemKind.Method) };
      }
      return {
        suggestions: mk(["nem", "PID", "EXPORT"], monaco.languages.CompletionItemKind.Variable),
      };
    },
  });
}
