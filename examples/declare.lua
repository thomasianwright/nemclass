-- Headless smoke test: declare a type and generate code. Needs no target.
-- Run: cargo run -p nemclass_cli -- run examples/declare.lua

print("processes visible: " .. #nem.processes())

local player = nem.class("Player")
player:field("health", nem.kinds.i32)
player:field("alive", nem.kinds.bool)
player:pad(3)                       -- alignment padding
player:field("pos", nem.kinds.vec(3, "f32"))
player:field("view", nem.kinds.mat(4, 4, "f32"))
player:field("name", nem.kinds.strptr)

local proj = nem.project()
proj:add(player:build())

print("---- Rust ----")
print(proj:generate("rust"))
print("---- C++ ----")
print(proj:generate("cpp"))
print("---- RON ----")
print(proj:to_ron())
