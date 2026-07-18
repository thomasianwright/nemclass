-- Attach to a target, scan a module for a signature, resolve an offset chain,
-- and read a value. Wine-aware: pointer width and module bases are handled by
-- the SDK.
--
-- Run against a running process, e.g.:
--   nemclass-cli run examples/scan.lua --name subject
--   nemclass-cli run examples/scan.lua --pid 1234

local proc = PID and nem.open(PID) or nem.attach{ name = PNAME }
print(string.format("attached to %s (pid %d)", proc:name(), proc:id()))
print(string.format("pointer size: %d bytes, wine: %s", proc:pointer_size(), tostring(proc:is_wine())))

-- Enumerate modules.
local mods = proc:modules()
print(string.format("%d modules loaded", #mods))
local main = mods[1]
print(string.format("first module: %s @ 0x%x (size 0x%x)", main.name, main.base, main.size))

-- IDA-style signature scan across the main module.
local sig = nem.pattern("48 8B ?? ?? ?? ?? ?? 48 89")
local hits = proc:scan(sig, { module = main.name })
print(string.format("signature hits in %s: %d", main.name, #hits))
if #hits > 0 then
  print(string.format("first hit: 0x%x", hits[1]))
end

-- Address-expression evaluation + pointer-chain resolution.
local base = proc:eval("<" .. main.name .. ">")
print(string.format("<%s> = 0x%x", main.name, base))

-- Example multi-level pointer resolve (offsets are placeholders).
local ok, addr = pcall(function() return proc:resolve(base, { 0x10, 0x0 }) end)
if ok then print(string.format("resolved 0x%x", addr)) end
