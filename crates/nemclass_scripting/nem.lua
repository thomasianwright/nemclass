---@meta
--- LuaCATS type definitions for the nemclass `nem` scripting API.
---
--- Point lua-language-server at this file to get autocompletion and hover docs
--- for `nem.*` in your scripts. New projects get a copy in `scripts/nem.lua`
--- plus a `.luarc.json`, so any editor with the Lua Language Server picks it up.
---
--- Reads/writes raise a Lua error when the address is unreadable/unwritable.

------------------------------------------------------------------------------
-- Data tables
------------------------------------------------------------------------------

---@class nem.ProcessInfo
---@field id integer
---@field name string
---@field parent_id integer

---@class nem.ModuleInfo
---@field base integer  Base address in the target's address space.
---@field size integer  Size in bytes (true SizeOfImage for Wine PE modules).
---@field name string

---@class nem.AttachOpts
---@field pid integer?      Attach by process id.
---@field name string?      Attach by (case-insensitive) image name.
---@field plugin string?    Path to a managed `yc_*` plugin library (with pid).

---@class nem.ScanOpts
---@field module string?    Scan this module by name.
---@field start integer?    Start address (with `len`).
---@field len integer?      Length in bytes (with `start`).

------------------------------------------------------------------------------
-- Target: a handle to a process's memory
------------------------------------------------------------------------------

---@class nem.Target
local Target = {}

---@param addr integer
---@return integer
function Target:read_i8(addr) end
---@param addr integer
---@return integer
function Target:read_i16(addr) end
---@param addr integer
---@return integer
function Target:read_i32(addr) end
---@param addr integer
---@return integer
function Target:read_i64(addr) end
---@param addr integer
---@return integer
function Target:read_u8(addr) end
---@param addr integer
---@return integer
function Target:read_u16(addr) end
---@param addr integer
---@return integer
function Target:read_u32(addr) end
---@param addr integer
---@return integer
function Target:read_u64(addr) end
---@param addr integer
---@return number
function Target:read_f32(addr) end
---@param addr integer
---@return number
function Target:read_f64(addr) end

---Read a pointer, honoring the target's pointer width (4 on WoW64/32-bit).
---@param addr integer
---@return integer
function Target:read_ptr(addr) end

---Read `len` raw bytes as a Lua string.
---@param addr integer
---@param len integer
---@return string
function Target:read_bytes(addr, len) end

---Read a NUL-terminated UTF-8 string (capped at 4 KiB).
---@param addr integer
---@return string
function Target:read_string(addr) end

---@param addr integer
---@param value integer
function Target:write_i8(addr, value) end
---@param addr integer
---@param value integer
function Target:write_i16(addr, value) end
---@param addr integer
---@param value integer
function Target:write_i32(addr, value) end
---@param addr integer
---@param value integer
function Target:write_i64(addr, value) end
---@param addr integer
---@param value integer
function Target:write_u8(addr, value) end
---@param addr integer
---@param value integer
function Target:write_u16(addr, value) end
---@param addr integer
---@param value integer
function Target:write_u32(addr, value) end
---@param addr integer
---@param value integer
function Target:write_u64(addr, value) end
---@param addr integer
---@param value number
function Target:write_f32(addr, value) end
---@param addr integer
---@param value number
function Target:write_f64(addr, value) end
---@param addr integer
---@param data string
function Target:write_bytes(addr, data) end

---@param addr integer
---@return boolean
function Target:can_read(addr) end

---@return integer
function Target:id() end
---@return string
function Target:name() end
---Pointer width in bytes (4 for 32-bit/WoW64, 8 for 64-bit).
---@return integer
function Target:pointer_size() end
---@return boolean
function Target:is_wine() end

---@return nem.ModuleInfo[]
function Target:modules() end
---@param name string
---@return nem.ModuleInfo
function Target:module(name) end

---Scan for `pat`. Pass `{ module = "game.exe" }` or `{ start = .., len = .. }`.
---@param pat nem.Pattern
---@param opts nem.ScanOpts
---@return integer[]  Match addresses.
function Target:scan(pat, opts) end
---@param pat nem.Pattern
---@param module string
---@return integer[]
function Target:scan_module(pat, module) end
---@param pat nem.Pattern
---@param start integer
---@param len integer
---@return integer[]
function Target:scan_range(pat, start, len) end

---Follow a multi-level pointer chain: `*(*(base+o1)+o2)+...`.
---@param base integer
---@param offsets integer[]
---@return integer
function Target:resolve(base, offsets) end

---Resolve a RIP-relative reference: read i32 at `rel32_addr`, add `next_insn`.
---@param rel32_addr integer
---@param next_insn integer
---@return integer
function Target:rip(rel32_addr, next_insn) end

---Evaluate an address expression, e.g. `["game.exe"+0x1A2B]+0x10`.
---@param expr string
---@return integer
function Target:eval(expr) end

---Best-guess field kind name at `addr` (e.g. "Vec3f", "Ptr"), or nil.
---@param addr integer
---@return string?
function Target:infer(addr) end

------------------------------------------------------------------------------
-- Pattern / Kind / class builder
------------------------------------------------------------------------------

---@class nem.Pattern
local Pattern = {}
---@return integer
function Pattern:len() end
---@return boolean
function Pattern:is_empty() end

---@class nem.Kind
local Kind = {}
---@return string
function Kind:name() end
---@return integer
function Kind:size() end

---@class nem.ClassBuilder
local ClassBuilder = {}
---Append a field at the running offset.
---@param name string
---@param kind nem.Kind
function ClassBuilder:field(name, kind) end
---Place a field at an explicit offset.
---@param name string
---@param kind nem.Kind
---@param offset integer
function ClassBuilder:field_at(name, kind, offset) end
---Advance the running offset (padding).
---@param bytes integer
function ClassBuilder:pad(bytes) end
---@return nem.Type
function ClassBuilder:build() end

---@class nem.Type
local Type = {}
---@return string
function Type:name() end
---@return string
function Type:to_ron() end
---@param lang string  "rust" | "cpp"
---@return string
function Type:generate(lang) end

---@class nem.Project
local Project = {}
---@param ty nem.Type
function Project:add(ty) end
---@return string
function Project:to_ron() end
---@param lang string  "rust" | "cpp"
---@return string
function Project:generate(lang) end

------------------------------------------------------------------------------
-- Field-kind constructors
------------------------------------------------------------------------------

---@class nem.Kinds
---@field i8 nem.Kind
---@field i16 nem.Kind
---@field i32 nem.Kind
---@field i64 nem.Kind
---@field u8 nem.Kind
---@field u16 nem.Kind
---@field u32 nem.Kind
---@field u64 nem.Kind
---@field f32 nem.Kind
---@field f64 nem.Kind
---@field bool nem.Kind
---@field ptr nem.Kind
---@field strptr nem.Kind
---@field hex8 nem.Kind
---@field hex16 nem.Kind
---@field hex32 nem.Kind
---@field hex64 nem.Kind
local Kinds = {}
---Float vector kind, e.g. `nem.kinds.vec(3, "f32")`.
---@param components integer
---@param width string  "f32" | "f64"
---@return nem.Kind
function Kinds.vec(components, width) end
---Float matrix kind, e.g. `nem.kinds.mat(4, 4, "f32")`.
---@param rows integer
---@param cols integer
---@param width string  "f32" | "f64"
---@return nem.Kind
function Kinds.mat(rows, cols, width) end

------------------------------------------------------------------------------
-- The `nem` global
------------------------------------------------------------------------------

---@class nem
---@field kinds nem.Kinds
nem = {}

---Enumerate running processes.
---@return nem.ProcessInfo[]
function nem.processes() end

---Attach to a process. `{ pid = .. }` or `{ name = .. }` (optional `plugin`).
---@param opts nem.AttachOpts
---@return nem.Target
function nem.attach(opts) end

---Attach natively by process id.
---@param pid integer
---@return nem.Target
function nem.open(pid) end

---Parse a signature. `style` is "ida" (default) or "peid".
---@param sig string
---@param style string?
---@return nem.Pattern
function nem.pattern(sig, style) end

---Build a pattern from a byte template and an `x`/`?` mask.
---@param bytes string
---@param mask string
---@return nem.Pattern
function nem.pattern_code(bytes, mask) end

---Start a class/type declaration.
---@param name string
---@return nem.ClassBuilder
function nem.class(name) end

---A new, empty project (collection of types).
---@return nem.Project
function nem.project() end

---Parse a project from RON.
---@param ron string
---@return nem.Project
function nem.load_project(ron) end

---Generate source for a project. `lang` is "rust" or "cpp".
---@param project nem.Project
---@param lang string
---@return string
function nem.generate(project, lang) end

------------------------------------------------------------------------------
-- GUI-only bindings (the GUI script console; error in the headless CLI)
------------------------------------------------------------------------------

---Names of the classes in the GUI's class list, in list order.
---@return string[]
function nem.classes() end

---Set the base address of a class in the GUI's inspector — e.g. after scanning
---for it — so the inspector updates automatically. Raises if there is no class
---with that name.
---@param name string
---@param address integer
function nem.set_class_address(name, address) end

---Current base address of a class, or nil if it doesn't exist.
---@param name string
---@return integer?
function nem.class_address(name) end

------------------------------------------------------------------------------
-- Host-provided globals (set by the GUI console / CLI)
------------------------------------------------------------------------------

---Attached process id, or nil.
---@type integer?
PID = nil

---Target process name from `--name`, or nil.
---@type string?
PNAME = nil

---Open project directory (CLI `--project`), or nil.
---@type string?
PROJECT = nil

---Set this to a project RON string to import the declared classes into the GUI.
---@type string?
EXPORT = nil
