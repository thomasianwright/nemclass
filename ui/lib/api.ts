// Typed wrappers over the Tauri command layer. Tauri converts camelCase JS keys
// to the backend's snake_case argument names automatically.
import { invoke } from "@tauri-apps/api/core";

// ---- DTOs (mirror src-tauri/src/dto.rs + schema serde) -------------------

export interface ProcessInfo {
  id: number;
  name: string;
  parentId: number;
}

export interface Attached {
  pid: number;
  name: string;
  pointerSize: number;
  isWine: boolean;
  isManaged: boolean;
}

export interface ClassSummary {
  name: string;
  fieldCount: number;
  size: number;
}

/** Matches nemclass_sdk::schema::FieldDef (default serde => snake_case). */
export interface FieldDef {
  name: string;
  offset: number;
  kind: string;
  metadata: string | null;
}

export interface TypeDef {
  name: string;
  fields: FieldDef[];
}

export interface KindOption {
  kind: string;
  size: number;
}

/** Matches nemclass_sdk::project::AutoAttach. */
export interface AutoAttach {
  process_name: string;
  module_name: string | null;
}

/** Matches nemclass_sdk::project::Manifest. */
export interface Manifest {
  name: string;
  auto_attach: AutoAttach | null;
}

export interface ProjectStatus {
  name: string;
  dir: string | null;
  dirty: boolean;
  classCount: number;
  attached: Attached | null;
}

export interface Config {
  recentProjects: string[];
  pollHz: number;
  layout: string | null;
}

export interface DirEntry {
  name: string;
  path: string;
  isProject: boolean;
}

export interface DirListing {
  path: string;
  parent: string | null;
  dirs: DirEntry[];
}

export interface CheatEntry {
  index: number;
  description: string;
  address: string;
  kind: string;
  resolved: number | null;
  value: string | null;
  frozen: boolean;
}

export interface Insn {
  addr: number;
  len: number;
  bytes: string;
  text: string;
  kind: string;
  target: number | null;
}

export interface MapRegion {
  from: number;
  to: number;
  size: number;
  read: boolean;
  write: boolean;
  exec: boolean;
  name: string;
  label: string;
  kind: string;
}

export interface StringHit {
  addr: number;
  text: string;
}

export interface ModuleInfo {
  base: number;
  size: number;
  name: string;
}

export interface ScanSummary {
  count: number;
  valueType: string;
}

export interface ScanRow {
  address: number;
  value: string | null;
  previous: string;
}

export interface Compare {
  op: string;
  value?: string | null;
  value2?: string | null;
}

export interface SpiderStatus {
  running: boolean;
  count: number;
}

export interface SpiderResult {
  expr: string;
  depth: number;
  address: number | null;
  value: string | null;
}

export interface Registers {
  rax: number; rbx: number; rcx: number; rdx: number;
  rsi: number; rdi: number; rbp: number; rsp: number; rip: number;
  r8: number; r9: number; r10: number; r11: number;
  r12: number; r13: number; r14: number; r15: number;
  eflags: number;
}

export interface DebugEvent {
  tid: number;
  reason: string;
  addr: number | null;
  bpId: number | null;
  exitCode: number | null;
}

export interface AccessRecord {
  insnAddr: number;
  hits: number;
  regs: Registers;
}

export interface ScriptResult {
  output: string;
  error: string | null;
  export: string | null;
  merged: boolean;
}

export interface FieldInput {
  name: string;
  offset: number;
  kind: string;
  metadata?: string | null;
}

export interface FieldRow {
  fieldIndex: number;
  offset: number;
  address: number;
  name: string;
  kind: string;
  size: number;
  value: string | null;
  kindMeta: string | null;
  pointee: number | null;
  expandable: boolean;
  children: FieldRow[];
}

export interface InspectResult {
  className: string;
  baseAddr: number;
  ptrSize: number;
  attached: boolean;
  rows: FieldRow[];
}

// ---- Command wrappers ----------------------------------------------------

export const api = {
  // process
  listProcesses: () => invoke<ProcessInfo[]>("list_processes"),
  attachPid: (pid: number) => invoke<Attached>("attach_pid", { pid }),
  attachName: (name: string) => invoke<Attached>("attach_name", { name }),
  attachManaged: (pid: number, plugin: string) =>
    invoke<Attached>("attach_managed", { pid, plugin }),
  autoAttach: () => invoke<Attached>("auto_attach"),
  detach: () => invoke<void>("detach"),
  attachStatus: () => invoke<Attached | null>("attach_status"),

  // classes / fields
  listClasses: () => invoke<ClassSummary[]>("list_classes"),
  getClass: (name: string) => invoke<TypeDef>("get_class", { name }),
  addClass: (name: string) => invoke<void>("add_class", { name }),
  renameClass: (oldName: string, newName: string) =>
    invoke<void>("rename_class", { old: oldName, new: newName }),
  deleteClass: (name: string) => invoke<void>("delete_class", { name }),
  addField: (cls: string, name: string, kind: string, metadata?: string | null) =>
    invoke<void>("add_field", { class: cls, name, kind, metadata: metadata ?? null }),
  setFieldName: (cls: string, index: number, name: string) =>
    invoke<void>("set_field_name", { class: cls, index, name }),
  setFieldKind: (cls: string, index: number, kind: string, metadata?: string | null) =>
    invoke<void>("set_field_kind", { class: cls, index, kind, metadata: metadata ?? null }),
  setFieldOffset: (cls: string, index: number, offset: number) =>
    invoke<void>("set_field_offset", { class: cls, index, offset }),
  deleteField: (cls: string, index: number) =>
    invoke<void>("delete_field", { class: cls, index }),
  insertFields: (cls: string, atIndex: number, fields: FieldInput[]) =>
    invoke<void>("insert_fields", { class: cls, atIndex, fields }),
  undo: () => invoke<boolean>("undo"),
  redo: () => invoke<boolean>("redo"),
  fieldKinds: () => invoke<KindOption[]>("field_kinds"),

  // inspector / memory
  inspectClass: (cls: string, expanded: number[][]) =>
    invoke<InspectResult>("inspect_class", { class: cls, expanded }),
  getClassAddress: (name: string) =>
    invoke<number>("get_class_address", { name }),
  setClassAddress: (name: string, addr: number) =>
    invoke<void>("set_class_address", { name, addr }),
  writeValue: (address: number, kind: string, text: string) =>
    invoke<void>("write_value", { address, kind, text }),
  readBytes: (address: number, len: number) =>
    invoke<number[]>("read_bytes", { address, len }),
  writeBytes: (address: number, bytes: number[]) =>
    invoke<void>("write_bytes", { address, bytes }),

  // project
  projectNew: (dir: string, name: string) =>
    invoke<ProjectStatus>("project_new", { dir, name }),
  projectOpen: (dir: string) => invoke<ProjectStatus>("project_open", { dir }),
  projectSave: () => invoke<ProjectStatus>("project_save"),
  projectSaveAs: (dir: string) => invoke<ProjectStatus>("project_save_as", { dir }),
  isProjectDir: (dir: string) => invoke<boolean>("is_project_dir", { dir }),
  getManifest: () => invoke<Manifest>("get_manifest"),
  setManifest: (manifest: Manifest) => invoke<void>("set_manifest", { manifest }),
  projectStatus: () => invoke<ProjectStatus>("project_status"),

  // config
  getConfig: () => invoke<Config>("get_config"),
  setConfig: (config: Config) => invoke<void>("set_config", { config }),
  setLayout: (layout: string | null) => invoke<void>("set_layout", { layout }),

  // filesystem (in-app directory picker)
  listDir: (path: string | null) => invoke<DirListing>("list_dir", { path }),
  homeDir: () => invoke<string>("home_dir"),

  // cheat table
  tableList: () => invoke<CheatEntry[]>("table_list"),
  tableAdd: (description: string, address: string, kind: string) =>
    invoke<void>("table_add", { description, address, kind }),
  tableUpdate: (index: number, description: string, address: string, kind: string) =>
    invoke<void>("table_update", { index, description, address, kind }),
  tableRemove: (index: number) => invoke<void>("table_remove", { index }),
  tableWrite: (index: number, text: string) =>
    invoke<void>("table_write", { index, text }),
  tableFreeze: (index: number, on: boolean) =>
    invoke<void>("table_freeze", { index, on }),

  // code generation
  generateCode: (lang: string) => invoke<string>("generate_code", { lang }),
  genLangs: () => invoke<string[]>("gen_langs"),

  // scanner
  scanFirst: (
    valueType: string,
    compare: Compare,
    writableOnly: boolean,
    alignment?: number | null,
  ) =>
    invoke<ScanSummary>("scan_first", {
      valueType,
      compare,
      writableOnly,
      alignment: alignment ?? null,
    }),
  scanNext: (compare: Compare) => invoke<ScanSummary>("scan_next", { compare }),
  scanReset: () => invoke<void>("scan_reset"),
  scanPage: (offset: number, limit: number) =>
    invoke<ScanRow[]>("scan_page", { offset, limit }),
  scanAddToTable: (index: number, description: string) =>
    invoke<void>("scan_add_to_table", { index, description }),

  // scripting
  scriptRun: (code: string) => invoke<ScriptResult>("script_run", { code }),
  scriptList: () => invoke<string[]>("script_list"),
  scriptLoad: (name: string) => invoke<string>("script_load", { name }),
  scriptSave: (name: string, code: string) =>
    invoke<void>("script_save", { name, code }),
  scriptDefinitions: () => invoke<string>("script_definitions"),

  // spider
  spiderSearch: (
    address: number,
    structSize: number,
    alignment: number,
    depth: number,
    kind: string,
    value: string,
  ) =>
    invoke<void>("spider_search", { address, structSize, alignment, depth, kind, value }),
  spiderStatus: () => invoke<SpiderStatus>("spider_status"),
  spiderPage: (offset: number, limit: number) =>
    invoke<SpiderResult[]>("spider_page", { offset, limit }),
  spiderFilter: (filter: string, value: string) =>
    invoke<number>("spider_filter", { filter, value }),
  spiderCancel: () => invoke<void>("spider_cancel"),
  spiderAddToTable: (index: number, description: string) =>
    invoke<void>("spider_add_to_table", { index, description }),

  // debugger + access tracer
  debuggerAttach: (backend: string) => invoke<void>("debugger_attach", { backend }),
  debuggerDetach: () => invoke<void>("debugger_detach"),
  debuggerStatus: () => invoke<string | null>("debugger_status"),
  debuggerThreads: () => invoke<number[]>("debugger_threads"),
  bpSetSw: (addr: number) => invoke<number>("bp_set_sw", { addr }),
  bpSetHw: (addr: number, size: number, kind: string) =>
    invoke<number>("bp_set_hw", { addr, size, kind }),
  bpClear: (id: number) => invoke<void>("bp_clear", { id }),
  dbgContinue: () => invoke<void>("dbg_continue"),
  dbgStep: (tid: number) => invoke<void>("dbg_step", { tid }),
  dbgRegisters: (tid: number) => invoke<Registers>("dbg_registers", { tid }),
  dbgSetRegisters: (tid: number, regs: Registers) =>
    invoke<void>("dbg_set_registers", { tid, regs }),
  accessStart: (addr: number, size: number, kind: string, backend: string) =>
    invoke<void>("access_start", { addr, size, kind, backend }),
  accessStop: () => invoke<void>("access_stop"),

  // disassembly / memory map
  memoryMap: () => invoke<MapRegion[]>("memory_map"),
  listModules: () => invoke<ModuleInfo[]>("list_modules"),
  disassemble: (start: number, count: number) =>
    invoke<Insn[]>("disassemble", { start, count }),
  regionStrings: (start: number, len: number, minLen: number) =>
    invoke<StringHit[]>("region_strings", { start, len, minLen }),
  regionFunctions: (start: number, len: number) =>
    invoke<number[]>("region_functions", { start, len }),
  regionCalls: (start: number, len: number) =>
    invoke<number[]>("region_calls", { start, len }),
};

/** Parses a hex/decimal address string (e.g. "0x1400", "5242880") to a number. */
export function parseAddr(text: string): number | null {
  const t = text.trim().toLowerCase().replace(/_/g, "");
  if (!t) return null;
  const n = t.startsWith("0x") ? parseInt(t.slice(2), 16) : parseInt(t, 10);
  return Number.isFinite(n) ? n : null;
}

/** Formats a number as a 0x-prefixed hex address. */
export function hex(n: number): string {
  return "0x" + (n >>> 0).toString(16).toUpperCase();
}

/** Formats a possibly-large address (uses BigInt-safe hex). */
export function hexAddr(n: number): string {
  return "0x" + Math.trunc(n).toString(16).toUpperCase();
}
