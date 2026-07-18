# nemclass_scripting

A **Lua 5.4** scripting host for nemclass, built on the headless
[`nemclass_sdk`](../nemclass_sdk). Scripts get a single global table, `nem`, for
process enumeration and attachment, memory reads/writes, IDA-style pattern
scanning, pointer/offset resolution (Wine-aware), and type declarations with
Rust/C++ code generation.

- [Running scripts](#running-scripts)
- [Editor autocompletion](#editor-autocompletion)
- [Quick start](#quick-start)
- [API reference](#api-reference)
  - [Enumeration & attachment](#enumeration--attachment)
  - [`Target` — reading & writing memory](#target--reading--writing-memory)
  - [Modules](#modules)
  - [Pattern scanning](#pattern-scanning)
  - [Offsets & address expressions](#offsets--address-expressions)
  - [Type inference](#type-inference)
  - [Type declarations & code generation](#type-declarations--code-generation)
  - [Field kinds (`nem.kinds`)](#field-kinds-nemkinds)
  - [GUI-only bindings](#gui-only-bindings)
- [Host-provided globals](#host-provided-globals)
- [Error handling](#error-handling)
- [Value & numeric notes](#value--numeric-notes)

## Running scripts

Scripts can run three ways:

**Headless CLI** ([`nemclass_cli`](../nemclass_cli)):

```sh
nemclass-cli run scan.lua --name subject           # attach by name inside the script
nemclass-cli run scan.lua --pid 1234
nemclass-cli run scan.lua --project ./my_project   # resolve from scripts/, auto-attach from manifest
```

**GUI console** — the *Script* button opens a console that runs `nem.*` scripts,
captures `print`, exposes the attached process id as `PID`, and merges a project
you assign to the `EXPORT` global into the live class list — updating same-named
classes in place, adding new ones, and keeping the rest (undoable). Scripts are
loaded from / saved to the open project's `scripts/` folder.

**Embedded** (Rust):

```rust
let engine = nemclass_scripting::ScriptEngine::new()?;
engine.run_str(r#" print(#nem.processes() .. " processes") "#)?;
engine.run_file("scan.lua")?;
```

## Editor autocompletion

The crate ships [`nem.lua`](nem.lua) — LuaCATS/EmmyLua type definitions for the
whole API. With the **Lua Language Server** (the VS Code "Lua" extension by
sumneko, or `lua_ls` in Neovim) you get completion and hover docs for `nem.*`.

New projects created by the GUI or `nemclass-cli init <dir>` get a copy at
`scripts/nem.lua` plus a `.luarc.json`, so autocompletion works out of the box.
To add it to an existing folder:

```sh
nemclass-cli init ./my_project
```

## Quick start

```lua
-- Attach (PID is set by the host when --pid/--name/--project is used).
local proc = PID and nem.open(PID) or nem.attach{ name = "game.exe" }
print(string.format("%s (pid %d), wine=%s, ptr=%d bytes",
  proc:name(), proc:id(), tostring(proc:is_wine()), proc:pointer_size()))

-- Find a code pattern in a module, then resolve a RIP-relative pointer.
local hits = proc:scan(nem.pattern("48 8B 05 ?? ?? ?? ?? 48 8B 00"), { module = "game.exe" })
if #hits > 0 then
  local g_world = proc:rip(hits[1] + 3, hits[1] + 7)   -- disp32 at +3, next insn at +7
  local player  = proc:resolve(g_world, { 0x18, 0x0 }) -- *(*(g_world+0x18)+0x0)
  print(string.format("health = %d", proc:read_i32(player + 0xF0)))
end

-- Declare a struct and generate code.
local c = nem.class("Player")
c:field("health", nem.kinds.i32)
c:field("pos", nem.kinds.vec(3, "f32"))
local proj = nem.project(); proj:add(c:build())
print(proj:generate("rust"))
```

## API reference

Every bare number in an **address expression** is hexadecimal; regular Lua
integer literals are decimal as usual. Addresses are Lua integers (see
[numeric notes](#value--numeric-notes)).

### Enumeration & attachment

| Call | Returns | Notes |
|------|---------|-------|
| `nem.processes()` | `{ {id, name, parent_id}, ... }` | All running processes. |
| `nem.attach{ pid = N }` | `Target` | Native attach by pid. |
| `nem.attach{ name = "game.exe" }` | `Target` | First process whose image name matches (case-insensitive). |
| `nem.attach{ pid = N, plugin = "path" }` | `Target` | Route memory through a managed `yc_*` plugin library. |
| `nem.open(pid)` | `Target` | Shorthand for a native attach by pid. |

Attachment raises a Lua error if no matching process is found.

### `Target` — reading & writing memory

Reads and writes **raise a Lua error** when the address is not
readable/writable, so wrap fallible accesses in `pcall` if you expect misses.

```lua
proc:read_i8(a)   proc:read_i16(a)  proc:read_i32(a)  proc:read_i64(a)   --> integer
proc:read_u8(a)   proc:read_u16(a)  proc:read_u32(a)  proc:read_u64(a)   --> integer
proc:read_f32(a)  proc:read_f64(a)                                       --> number
proc:read_ptr(a)                        --> integer (width-aware: 4 bytes on WoW64)
proc:read_bytes(a, len)                 --> string (raw bytes)
proc:read_string(a)                     --> string (NUL-terminated UTF-8, ≤ 4 KiB)

proc:write_i8(a, v)  ... proc:write_i64(a, v)        -- integer value
proc:write_u8(a, v)  ... proc:write_u64(a, v)
proc:write_f32(a, v) proc:write_f64(a, v)            -- number value
proc:write_bytes(a, "\x90\x90")                      -- raw bytes

proc:can_read(a)        --> boolean
proc:id()               --> integer
proc:name()             --> string  ("[managed]" for plugin targets)
proc:pointer_size()     --> integer (4 for 32-bit / WoW64, 8 for 64-bit)
proc:is_wine()          --> boolean
```

### Modules

```lua
proc:modules()          --> { {base, size, name}, ... }
proc:module("game.exe") --> {base, size, name}   (raises if not found)
```

Wine PE images (fragmented across many `/proc/<pid>/maps` section mappings,
often under paths containing spaces) are recognised; `base` is the true image
base and `size` is the PE `SizeOfImage`.

### Pattern scanning

```lua
local pat = nem.pattern("48 8B ?? ?? 48 89")          -- IDA style (default)
local pat = nem.pattern("48 8B ?? ?? 48 89", "peid")  -- PEID style (?? wildcards)
local pat = nem.pattern_code("\x48\x8B\x00\x00", "xx??")  -- byte template + mask

pat:len()        --> integer
pat:is_empty()   --> boolean

proc:scan(pat, { module = "game.exe" })      --> { addr, ... }
proc:scan(pat, { start = 0x400000, len = 0x1000 })
proc:scan_module(pat, "game.exe")            --> { addr, ... }
proc:scan_range(pat, start, len)             --> { addr, ... }
```

`scan`/`scan_module`/`scan_range` return a (possibly empty) array of match
addresses.

### Offsets & address expressions

```lua
-- Multi-level pointer chain: *(*(base + 0x10) + 0x20) + 0x0
proc:resolve(base, { 0x10, 0x20, 0x0 })   --> integer

-- RIP-relative: read the signed disp32 at `rel32_addr`, add `next_insn_addr`.
proc:rip(rel32_addr, next_insn_addr)      --> integer

-- Address-expression evaluator.
proc:eval('["game.exe"+0x1A2B]+0x10')     --> integer
```

**Expression grammar** (all bare numbers are hex; `0x` optional):

| Form | Meaning |
|------|---------|
| `0x1234` / `1234` | a number |
| `"game.exe"` or `<game.exe>` | the module's base address |
| `[expr]` | dereference (width-aware pointer read) |
| `(expr)` | grouping |
| `+` `-` `*` | wrapping arithmetic |

Example: `["game.exe"+0x1A2B]+0x10` — read the pointer at `game.exe + 0x1A2B`,
then add `0x10`.

### Type inference

```lua
proc:infer(addr)   --> string | nil   -- best-guess kind name, e.g. "Vec3f", "Ptr", "F32"
```

Heuristic: prefers a valid (string) pointer, then a multi-lane float vector, then
a scalar float. Returns `nil` when nothing beats raw bytes.

### Type declarations & code generation

Build class layouts and emit Rust/C++ or RON.

```lua
local c = nem.class("Player")     -- a ClassBuilder
c:field("health", nem.kinds.i32)  -- append at the running offset
c:field("pos", nem.kinds.vec(3, "f32"))
c:field_at("flags", nem.kinds.u32, 0x40)   -- explicit offset
c:pad(4)                                    -- advance the running offset
c:field("target", nem.kinds.ptr)
local player = c:build()          -- a Type

player:name()            --> "Player"
player:to_ron()          --> string (RON)
player:generate("rust")  --> string

local proj = nem.project()        -- a Project (collection of Types)
proj:add(player)
proj:to_ron()               --> string
proj:generate("rust")       --> string   ("rust" | "cpp")
nem.generate(proj, "cpp")   --> string   (same as proj:generate)

local proj = nem.load_project(ron_string)   -- parse a Project from RON
```

In the **GUI console**, assign a project's RON to the `EXPORT` global to merge
the declared classes into the live class list. A class whose name already exists
is **updated in place** (its fields are replaced, its address kept); a new name
is **added**; classes you don't mention are **left untouched**:

```lua
EXPORT = proj:to_ron()
```

So to update one class, export just that class — the others survive. Note there
is no binding to read an existing class's fields back, so a class you redefine is
replaced wholesale rather than field-merged.

### Field kinds (`nem.kinds`)

Values passed to `class:field(...)`:

| Kind | Values |
|------|--------|
| Signed int | `nem.kinds.i8` `i16` `i32` `i64` |
| Unsigned int | `nem.kinds.u8` `u16` `u32` `u64` |
| Float | `nem.kinds.f32` `f64` |
| Bool / pointers | `nem.kinds.bool` `ptr` `strptr` |
| Raw hex | `nem.kinds.hex8` `hex16` `hex32` `hex64` |
| Vector | `nem.kinds.vec(components, "f32"|"f64")` e.g. `vec(3, "f32")` |
| Matrix | `nem.kinds.mat(rows, cols, "f32"|"f64")` e.g. `mat(4, 4, "f32")` |

A kind value also has `:name()` (e.g. `"Vec3f"`) and `:size()` (bytes).

### GUI-only bindings

These are available **only in the GUI script console** (they reach into the
live class list); in the headless CLI they raise a clear error. Use them to
automate the inspector — e.g. scan for a structure and point a class at it.

```lua
nem.classes()                          --> { "Player", "Enemy", ... }
nem.class_address("Player")            --> integer | nil (current base)
nem.set_class_address("Player", addr)  -- move the class's base (raises if no such class)
```

Example — find the local player and drive the `Player` class:

```lua
local proc = nem.open(PID)
local hits = proc:scan(nem.pattern("48 8B 05 ?? ?? ?? ?? 48 85 C0"), { module = "game.exe" })
local player = proc:read_ptr(proc:rip(hits[1] + 3, hits[1] + 7))
nem.set_class_address("Player", player)   -- inspector now shows the player struct
```

## Host-provided globals

The host (CLI / GUI console) sets these before running a script:

| Global | Type | Set when |
|--------|------|----------|
| `PID` | integer \| nil | `--pid`, or a project's resolved auto-attach |
| `PNAME` | string \| nil | `--name` |
| `PROJECT` | string \| nil | `--project DIR` (the project directory) |
| `EXPORT` | string (write) | *you* set it; the GUI console merges it into the class list (update-or-add by name) |

A portable attach line:

```lua
local proc = PID and nem.open(PID) or nem.attach{ name = PNAME }
```

## Error handling

- Failed reads/writes, a missing module, a bad pattern, an unresolvable pointer
  chain, or an invalid address expression **raise a Lua error**. Use `pcall`
  where a miss is expected:

  ```lua
  local ok, val = pcall(function() return proc:read_i32(addr) end)
  if ok then print(val) else print("unreadable") end
  ```

- `proc:scan*` return an empty table (not an error) when there are no matches.
- `proc:infer` returns `nil` (not an error) when it has no guess.

## Value & numeric notes

- Lua 5.4 has native 64-bit integers, so addresses and integer reads are exact.
  Unsigned 64-bit values above `2^63` wrap into negative Lua integers.
- Float reads/writes use Lua numbers (f64).
- `read_bytes` / `write_bytes` use Lua strings as raw byte buffers.
- Pointer reads (`read_ptr`, `[...]` in expressions, `resolve`) honour the
  target's pointer width, so 32-bit / WoW64 targets read 4-byte pointers.
